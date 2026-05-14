type ViewHeaderProps = {
  title: string;
  description: string;
};

export default function ViewHeader({ title, description }: ViewHeaderProps) {
  return (
    <div className="playground-view-header">
      <div>
        <h2>{title}</h2>
        <p>{description}</p>
      </div>
    </div>
  );
}
