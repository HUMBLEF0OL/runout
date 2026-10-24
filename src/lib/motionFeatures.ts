// Motion's layout/drag features (domMax), behind a dynamic import so they load with the page that needs them (Backlog
// reorder glide), not at start-up. Motion's documented split: a local module re-exporting the feature bundle.
export { domMax } from "motion/react";
