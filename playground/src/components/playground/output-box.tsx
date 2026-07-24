import JsonCodeEditor from '@/components/json-code-editor';
import type { RunState } from '../../types';

type OutputBoxProps = {
  runState: RunState;
  outputJson: string;
};

export default function OutputBox({ runState, outputJson }: OutputBoxProps) {
  if (!outputJson) {
    const emptyMessage = runState === 'running'
      ? 'Waiting for workflow output...'
      : runState === 'failed'
        ? 'The workflow failed before producing final output.'
        : runState === 'cancelled'
          ? 'The workflow was cancelled before producing final output.'
          : 'Run a workflow to see output.';

    return <div className="empty-state compact" role="status">{emptyMessage}</div>;
  }

  return <JsonCodeEditor value={outputJson} readOnly fullEditor uncappedHeight ariaLabel="Final workflow output" className="workflow-output__json" />;
}
