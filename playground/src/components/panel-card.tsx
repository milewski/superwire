import type { ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { cn } from '@/lib/utils';

type PanelCardProps = {
  title: string;
  description?: string;
  children: ReactNode;
  actions?: ReactNode;
  className?: string;
  bodyClassName?: string;
  collapsible?: boolean;
  open?: boolean;
  onToggle?: () => void;
};

export default function PanelCard({
  title,
  description,
  children,
  actions,
  className,
  bodyClassName,
  collapsible = false,
  open,
  onToggle,
}: PanelCardProps) {
  if (!collapsible) {
    return (
      <Card className={cn('panel-card', className)}>
        <div className="panel-card__header">
          <div className="panel-card__title-block">
            <strong>{title}</strong>
            {description ? <small>{description}</small> : null}
          </div>
          {actions ? <div className="panel-card__header-actions">{actions}</div> : null}
        </div>
        <CardContent className={cn('panel-card__body', bodyClassName)}>{children}</CardContent>
      </Card>
    );
  }

  return (
    <Collapsible open={open} onOpenChange={onToggle} asChild>
      <Card className={cn('panel-card', className)}>
        <div className="panel-card__header panel-card__header--collapsible">
          <CollapsibleTrigger asChild>
            <Button type="button" variant="ghost" className="panel-card__trigger" size="default">
              <span className="panel-card__title-block">
                <strong>{title}</strong>
                {description ? <small>{description}</small> : null}
              </span>
              <span className="panel-card__action" aria-hidden="true">
                <span>{open ? 'Collapse' : 'Expand'}</span>
                <ChevronDown />
              </span>
            </Button>
          </CollapsibleTrigger>
          {actions ? <div className="panel-card__header-actions">{actions}</div> : null}
        </div>
        <CollapsibleContent>
          <CardContent className={cn('panel-card__body', bodyClassName)}>{children}</CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  );
}
