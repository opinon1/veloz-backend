// Register the Chart.js pieces we use, once, at module load.
import {
  Chart,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  BarElement,
  ArcElement,
  Tooltip,
  Legend,
} from "chart.js";

Chart.register(
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  BarElement,
  ArcElement,
  Tooltip,
  Legend,
);

export const PALETTE = [
  "#4f9dff",
  "#3fd07f",
  "#ffb648",
  "#ff5f6e",
  "#b98bff",
  "#34d3c6",
  "#f06ec0",
  "#9aa7b8",
];
