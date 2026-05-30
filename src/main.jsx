import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// NOTE: StrictMode intentionally omitted. In dev it double-invokes effects,
// which double-registered the drag-drop listener and caused files to be saved
// twice on a single drop.
ReactDOM.createRoot(document.getElementById("root")).render(<App />);
