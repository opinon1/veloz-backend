//! Transactional email via an SQS-backed queue + SES v2 sender.
//!
//! Architecture (decoupled from the request path):
//!
//! ```text
//!   handler ──dispatch()──▶ tokio::spawn ──enqueue()──▶ SQS (veloz-emails)
//!                                                          │
//!   main.rs worker ──poll_once()──▶ ReceiveMessage ◀──────┘
//!        │ send_one() OK   ──▶ DeleteMessage
//!        │ send_one() Err  ──▶ leave msg ─▶ visibility timeout ─▶ redelivery
//!        └ after maxReceiveCount failures ─▶ DLQ (veloz-emails-dlq)
//! ```
//!
//! Why SQS rather than sending inline:
//!   - The purchase/signup request never blocks on SES, and an SES outage
//!     can never fail a purchase. The producer is fire-and-forget.
//!   - Retries are free: a failed send leaves the message on the queue and
//!     SQS redelivers it after the visibility timeout. A poison message
//!     lands in the DLQ after `maxReceiveCount` attempts.
//!
//! Disabled cleanly when `SQS_EMAIL_QUEUE_URL` is unset (local dev): the
//! `Mailer` is `None` in `AppState` and `dispatch` becomes a no-op.

use aws_config::BehaviorVersion;
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

use crate::state::AppState;

/// One queued email: the recipient plus a typed payload describing what to
/// render. Serialized to JSON as the SQS message body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailJob {
    pub to: String,
    pub kind: EmailKind,
}

/// Closed set of transactional emails. Add a variant here and implement its
/// arm in `render` — the compiler enforces the rest. Tagged so the JSON wire
/// format stays self-describing on the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmailKind {
    /// Sent after a successful signup.
    Welcome { username: String },
    /// Sent after a successful store purchase.
    PurchaseReceipt {
        item_name: String,
        cost: i64,
        currency: String,
        new_balance: i64,
    },
    /// Sent after a refund is issued. (No refund flow wired yet — the
    /// template is ready for when one lands.)
    Refund {
        item_name: String,
        amount: i64,
        currency: String,
    },
}

/// A rendered message ready to hand to SES.
struct Rendered {
    subject: String,
    html: String,
    text: String,
}

impl EmailKind {
    fn render(&self) -> Rendered {
        match self {
            EmailKind::Welcome { username } => Rendered {
                subject: "Welcome to Veloz".to_string(),
                html: format!(
                    "<h1>Welcome, {u}!</h1><p>Your Veloz account is ready. \
                     Jump in and start your first run.</p>",
                    u = esc(username)
                ),
                text: format!(
                    "Welcome, {username}!\n\nYour Veloz account is ready. \
                     Jump in and start your first run."
                ),
            },
            EmailKind::PurchaseReceipt {
                item_name,
                cost,
                currency,
                new_balance,
            } => Rendered {
                subject: format!("Your Veloz receipt — {item_name}"),
                html: format!(
                    "<h1>Thanks for your purchase</h1>\
                     <p>You bought <strong>{item}</strong>.</p>\
                     <p>Charged: <strong>{cost} {cur}</strong><br>\
                     Remaining balance: <strong>{bal} {cur}</strong></p>",
                    item = esc(item_name),
                    cost = cost,
                    cur = esc(currency),
                    bal = new_balance,
                ),
                text: format!(
                    "Thanks for your purchase!\n\nItem: {item_name}\n\
                     Charged: {cost} {currency}\nRemaining balance: {new_balance} {currency}"
                ),
            },
            EmailKind::Refund {
                item_name,
                amount,
                currency,
            } => Rendered {
                subject: format!("Your Veloz refund — {item_name}"),
                html: format!(
                    "<h1>Refund issued</h1>\
                     <p>We refunded <strong>{amt} {cur}</strong> for \
                     <strong>{item}</strong>.</p>",
                    amt = amount,
                    cur = esc(currency),
                    item = esc(item_name),
                ),
                text: format!(
                    "Refund issued.\n\nItem: {item_name}\nRefunded: {amount} {currency}"
                ),
            },
        }
    }
}

/// Minimal HTML-escaping for user-controlled strings (username, item names)
/// interpolated into the HTML body. Keeps a stray `<` from breaking markup.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Clone)]
pub struct Mailer {
    sqs: aws_sdk_sqs::Client,
    ses: aws_sdk_sesv2::Client,
    queue_url: String,
    from: String,
}

impl Mailer {
    /// Build from env. Returns `None` (feature disabled) when
    /// `SQS_EMAIL_QUEUE_URL` is unset/empty — keeps local dev runnable with
    /// no AWS credentials.
    ///
    ///   SQS_EMAIL_QUEUE_URL  — required; presence enables the feature
    ///   EMAIL_FROM           — sender address (default noreply@velozthegame.com)
    ///   SES_REGION           — optional SES region override (else AWS_REGION)
    pub async fn from_env() -> Option<Self> {
        let queue_url = env::var("SQS_EMAIL_QUEUE_URL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let from = env::var("EMAIL_FROM")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "noreply@velozthegame.com".to_string());

        // Shared config (region + credentials) from the standard provider
        // chain. On Fargate this resolves the task IAM role automatically.
        let shared = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let sqs = aws_sdk_sqs::Client::new(&shared);

        // SES may live in a different region than SQS if needed.
        let ses = match env::var("SES_REGION").ok().filter(|s| !s.is_empty()) {
            Some(region) => {
                let ses_cfg = aws_sdk_sesv2::config::Builder::from(&shared)
                    .region(aws_sdk_sesv2::config::Region::new(region))
                    .build();
                aws_sdk_sesv2::Client::from_conf(ses_cfg)
            }
            None => aws_sdk_sesv2::Client::new(&shared),
        };

        Some(Self {
            sqs,
            ses,
            queue_url,
            from,
        })
    }

    /// Producer: serialize the job and put it on the queue.
    pub async fn enqueue(&self, job: &EmailJob) -> Result<(), String> {
        let body = serde_json::to_string(job).map_err(|e| e.to_string())?;
        self.sqs
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Consumer tick: long-poll for up to 10 messages, send each, and delete
    /// only the ones that send successfully. A failed send is left on the
    /// queue for SQS to redeliver after the visibility timeout. Blocks up to
    /// 20s waiting for messages, so the caller can loop tightly without
    /// busy-spinning.
    pub async fn poll_once(&self) {
        let received = self
            .sqs
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(20)
            .send()
            .await;

        let msgs = match received {
            Ok(out) => out.messages.unwrap_or_default(),
            Err(e) => {
                tracing::warn!("email worker: receive failed: {e}");
                // Brief backoff so a persistent receive error doesn't hot-loop.
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                return;
            }
        };

        for msg in msgs {
            let Some(body) = msg.body() else { continue };
            let job: EmailJob = match serde_json::from_str(body) {
                Ok(j) => j,
                Err(e) => {
                    // Unparseable message: delete it so it doesn't loop to the
                    // DLQ — there's no recovering a malformed body.
                    tracing::error!("email worker: bad job body, dropping: {e}");
                    self.delete(msg.receipt_handle()).await;
                    continue;
                }
            };

            match self.send_one(&job).await {
                Ok(()) => self.delete(msg.receipt_handle()).await,
                Err(e) => {
                    // Leave on queue: SQS redelivers after the visibility
                    // timeout; the DLQ catches it after maxReceiveCount.
                    tracing::warn!("email worker: send to {} failed: {e}", job.to);
                }
            }
        }
    }

    async fn delete(&self, receipt_handle: Option<&str>) {
        let Some(rh) = receipt_handle else { return };
        if let Err(e) = self
            .sqs
            .delete_message()
            .queue_url(&self.queue_url)
            .receipt_handle(rh)
            .send()
            .await
        {
            tracing::warn!("email worker: delete failed: {e}");
        }
    }

    /// Render + send a single job through SES v2.
    async fn send_one(&self, job: &EmailJob) -> Result<(), String> {
        use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};

        let r = job.kind.render();

        let subject = Content::builder()
            .data(r.subject)
            .charset("UTF-8")
            .build()
            .map_err(|e| e.to_string())?;
        let html = Content::builder()
            .data(r.html)
            .charset("UTF-8")
            .build()
            .map_err(|e| e.to_string())?;
        let text = Content::builder()
            .data(r.text)
            .charset("UTF-8")
            .build()
            .map_err(|e| e.to_string())?;

        let body = Body::builder().html(html).text(text).build();
        let message = Message::builder().subject(subject).body(body).build();
        let content = EmailContent::builder().simple(message).build();
        let dest = Destination::builder()
            .to_addresses(&job.to)
            .build();

        self.ses
            .send_email()
            .from_email_address(&self.from)
            .destination(dest)
            .content(content)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Fire-and-forget producer for a known recipient. Spawns the enqueue so the
/// caller's response is never delayed and an SQS hiccup never bubbles up. No-op
/// when the mailer is disabled.
pub fn dispatch_to(state: &AppState, to: String, kind: EmailKind) {
    let Some(mailer) = state.mailer.clone() else {
        return;
    };
    tokio::spawn(async move {
        let job = EmailJob { to, kind };
        if let Err(e) = mailer.enqueue(&job).await {
            tracing::warn!("email enqueue failed: {e}");
        }
    });
}

/// Fire-and-forget producer that resolves the user's email first. Use from
/// handlers that have a `user_id` but not the address. No-op when disabled.
pub fn dispatch_to_user(state: &AppState, user_id: Uuid, kind: EmailKind) {
    if state.mailer.is_none() {
        return;
    }
    let db = state.db.clone();
    let state = state.clone();
    tokio::spawn(async move {
        let row: Result<Option<(String,)>, _> =
            sqlx::query_as("SELECT email FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&db)
                .await;
        match row {
            Ok(Some((to,))) => dispatch_to(&state, to, kind),
            Ok(None) => tracing::warn!("email: no user {user_id} to mail"),
            Err(e) => tracing::warn!("email: lookup for {user_id} failed: {e}"),
        }
    });
}
