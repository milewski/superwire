import { useRef, type DragEvent, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';

export interface PlaygroundTabChipAction {
  label: string;
  icon: ReactNode;
  onClick: () => void;
}

interface PlaygroundTabChipProps {
  size: 'large' | 'small';
  active: boolean;
  tone?: 'default' | 'error';
  activeGlow?: boolean;
  draggable?: boolean;
  dragging: boolean;
  dragOver: boolean;
  trigger: ReactNode;
  actions: PlaygroundTabChipAction[];
  onDragStart?: () => void;
  onDragOver?: () => void;
  onDrop?: () => void;
  onDragEnd?: () => void;
}

export default function PlaygroundTabChip({
  size,
  active,
  tone = 'default',
  activeGlow = false,
  draggable = true,
  dragging,
  dragOver,
  trigger,
  actions,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
}: PlaygroundTabChipProps) {
  const dragPreviewRef = useRef<HTMLElement | null>(null);

  function handleDragStart(dragEvent: DragEvent<HTMLDivElement>) {
    if (!draggable) {
      return;
    }

    dragEvent.dataTransfer.effectAllowed = 'move';
    dragEvent.dataTransfer.setData('text/plain', '');
    setDragPreview(dragEvent);
    onDragStart?.();
  }

  function handleDragEnd() {
    removeDragPreview();
    onDragEnd?.();
  }

  function setDragPreview(dragEvent: DragEvent<HTMLDivElement>) {
    removeDragPreview();

    const dragPreview = dragEvent.currentTarget.cloneNode(true) as HTMLElement;
    dragPreview.classList.add('playground-tab-chip--drag-preview');
    dragPreview.setAttribute('data-dragging', 'false');
    dragPreview.setAttribute('data-drag-over', 'false');
    dragPreview.style.width = `${dragEvent.currentTarget.offsetWidth}px`;
    document.body.appendChild(dragPreview);
    dragPreviewRef.current = dragPreview;

    dragEvent.dataTransfer.setDragImage(
      dragPreview,
      dragEvent.currentTarget.offsetWidth / 2,
      dragEvent.currentTarget.offsetHeight / 2,
    );

    window.requestAnimationFrame(() => {
      if (dragPreviewRef.current === dragPreview) {
        dragPreview.style.top = '-10000px';
        dragPreview.style.left = '-10000px';
      }
    });
  }

  function removeDragPreview() {
    dragPreviewRef.current?.remove();
    dragPreviewRef.current = null;
  }

  return (
    <div
      className="playground-tab-chip"
      draggable={draggable}
      data-size={size}
      data-active={active ? 'true' : 'false'}
      data-tone={tone}
      data-active-glow={activeGlow ? 'true' : 'false'}
      data-draggable={draggable ? 'true' : 'false'}
      data-dragging={dragging ? 'true' : 'false'}
      data-drag-over={dragOver ? 'true' : 'false'}
      onDragStart={draggable ? handleDragStart : undefined}
      onDragOver={draggable ? (dragEvent) => {
        dragEvent.preventDefault();
        onDragOver?.();
      } : undefined}
      onDrop={draggable ? (dragEvent) => {
        dragEvent.preventDefault();
        onDrop?.();
      } : undefined}
      onDragEnd={draggable ? handleDragEnd : undefined}
    >
      {trigger}

      {actions.length > 0 ? (
        <div className="playground-tab-chip__actions">
          {actions.map((action) => (
            <Button
              key={action.label}
              className="playground-tab-chip__action"
              variant="ghost"
              size="icon-sm"
              aria-label={action.label}
              onClick={action.onClick}
            >
              {action.icon}
            </Button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
