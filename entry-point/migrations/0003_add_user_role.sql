ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';
-- role values: 'user' | 'admin'
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
