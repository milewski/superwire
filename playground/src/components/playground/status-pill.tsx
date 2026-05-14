import { Badge } from '@/components/ui/badge';
import type { ValidationState } from '../../types';

type StatusPillProps = {
  state: ValidationState;
};

export default function StatusPill({ state }: StatusPillProps) {
  return <Badge variant="outline" className={`status-pill ${state}`}>{state}</Badge>;
}
