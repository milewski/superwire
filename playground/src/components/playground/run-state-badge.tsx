import { Loader2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import type { RunState } from '../../types';

type RunStateBadgeProps = {
  state: RunState;
};

export default function RunStateBadge({ state }: RunStateBadgeProps) {
  return (
    <Badge variant="outline" className={`mini-status ${state}`} role="status" aria-label={`Run state: ${state}`} aria-live={state === 'running' ? 'polite' : 'off'}>
      {state === 'running' ? <Loader2 className="mini-status-spinner" /> : null}
      <span>{state}</span>
    </Badge>
  );
}
