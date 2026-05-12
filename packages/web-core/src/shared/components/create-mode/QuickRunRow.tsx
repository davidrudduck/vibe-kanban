// Quick Run creates an isolated worktree workspace without a kanban card.
// Phase 4 will add the Worktree / Main Folder isolation toggle here
// once the use_main_folder ADR is approved.
export function QuickRunRow() {
  return (
    <p className="text-xs text-low">
      Runs in an isolated workspace branch — no kanban card created.
    </p>
  );
}
