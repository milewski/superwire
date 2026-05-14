import JsonCodeEditor from '@/components/json-code-editor';
import type { RunState } from '../../types';

type OutputBoxProps = {
  runState: RunState;
  outputJson: string;
};

export default function OutputBox({ runState, outputJson }: OutputBoxProps) {
  if (!outputJson) {
    return <div className="empty-state compact">{runState === 'running' ? 'Waiting for workflow output...' : 'Run a workflow to see output.'}</div>;
  }

  return <JsonCodeEditor value={outputJson} readOnly className="workflow-output__json" />;
}
