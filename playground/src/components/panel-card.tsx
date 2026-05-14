import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { cn } from '@/lib/utils';

type PanelCardProps = {
  title: string;
  description?: string;
  children: ReactNode;
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
  className,
  bodyClassName,
  collapsible = false,
  open,
  onToggle,
}: PanelCardProps) {
  if (!collapsible) {
    return (
      <Card className={cn('panel-card', className)}>
        <div className="panel-card-header">
          <div className="panel-card-title-block">
            <strong>{title}</strong>
            {description ? <small>{description}</small> : null}
          </div>
        </div>
        <CardContent className={cn('panel-card-body', bodyClassName)}>{children}</CardContent>
      </Card>
    );
  }

  return (
    <Collapsible open={open} onOpenChange={onToggle} asChild>
      <Card className={cn('panel-card', className)}>
        <CollapsibleTrigger asChild>
          <Button variant="ghost" className="panel-card-trigger" size="default">
            <span className="panel-card-title-block">
              <strong>{title}</strong>
              {description ? <small>{description}</small> : null}
            </span>
            <span>{open ? 'Collapse' : 'Expand'}</span>
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className={cn('panel-card-body', bodyClassName)}>{children}</CardContent>
        </CollapsibleContent>
      </Card>
    </Collapsible>
  );
}
