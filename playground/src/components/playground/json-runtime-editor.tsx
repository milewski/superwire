import { Button } from '@/components/ui/button';
import JsonCodeEditor from '@/components/json-code-editor';

type JsonRuntimeEditorProps = {
  title: string;
  value: string;
  secret?: boolean;
  validationError: string | null;
  onChange: (value: string) => void;
  onFormat: () => void;
};

export default function JsonRuntimeEditor({ title, value, secret, validationError, onChange, onFormat }: JsonRuntimeEditorProps) {
  return (
    <section className="runtime-json-editor" aria-label={`${title} JSON editor`}>
      <span className="runtime-json-editor__header">
        <span>
          <strong>{title}</strong>
          <small>{secret ? 'Sent as secrets.' : 'Sent as workflow input.'}</small>
        </span>
        <Button type="button" variant="outline" size="sm" onClick={onFormat}>Format JSON</Button>
      </span>
      <JsonCodeEditor value={value} onChange={onChange} />
      <em className={validationError ? 'runtime-json-editor__status runtime-json-editor__status--error' : 'runtime-json-editor__status runtime-json-editor__status--ok'}>
        {validationError ?? 'Valid JSON object'}
      </em>
    </section>
  );
}
