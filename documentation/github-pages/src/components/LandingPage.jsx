import WebGLWorkflowScene from "./WebGLWorkflowScene.jsx";

const workflowLines = [
  { keyword: "provider", text: " openai ", accent: "from", suffix: " openai {" },
  { text: "  endpoint: \"http://100.118.249.48:3000/v1\"" },
  { text: "  api_key: \"sk-S2Wcf15cJnGGhFpTHJHcClDmQoR6IwTx1PNl9cmIZF6Wtuxz\"" },
  { text: "}" },
  { text: "" },
  { keyword: "model", text: " openai_model ", accent: "from", suffix: " openai {" },
  { text: "  id: \"big-pickle\"" },
  { text: "}" },
  { text: "" },
  { keyword: "mcp", text: " local {" },
  { text: "  endpoint: \"http://localhost:8000/mcp/summarizer\"" },
  { text: "  headers {" },
  { text: "    Accept: \"application/json\"" },
  { text: "    Authorization: \"Bearer 788|McjZkNN0cJrNNfwGBqDMrt7X8Pg08Niq1MXkyP81d3b9dce1\"" },
  { text: "  }" },
  { text: "}" },
  { text: "" },
  { keyword: "from", text: " mcp.local {" },
  { text: "  bindings {" },
  { text: "    project_id: 14" },
  { text: "    task_id: 109" },
  { text: "  }" },
  { text: "}" },
  { text: "" },
  { keyword: "prompt", text: " dynamic_summary_prompt {" },
  { text: "  bindings {" },
  { text: "    project_id: 14" },
  { text: "    type: \"task\"" },
  { text: "    type_id: 109" },
  { text: "  }" },
  { text: "}" },
  { text: "" },
  { keyword: "tool", text: " list_all_participants_who_has_answered_given_task" },
  { keyword: "tool", text: " fetch_participant_answer" },
  { text: "}" },
  { text: "" },
  { keyword: "agent", text: " greeting {" },
  { text: "  model: model.openai_model" },
  { text: "  uses: [tool.list_all_participants_who_has_answered_given_task, prompt.dynamic_summary_prompt]" },
  { text: "" },
  { text: "  instruction: \"\"\"" },
  { text: "    call the prompt to figure out extra instructions user my request" },
  { text: "    Please analyze all tasks of the participants and provide me a summary" },
  { text: "  \"\"\"" },
  { text: "" },
  { keyword: "output", text: " {" },
  { text: "    summary: string" },
  { text: "  }" },
  { text: "}" },
  { text: "" },
  { keyword: "output", text: " {" },
  { text: "  greeting: agent.greeting.summary" },
  { text: "}" }
];

export default function LandingPage() {
  return (
    <main className="landing-page">
      <WebGLWorkflowScene />

      <section className="hero-layout" aria-labelledby="page-title">
        <div className="hero-copy">
          <img className="logo" src="/logo-horizontal.svg" alt="Superwire" />

          <h1 id="page-title">
            Turn AI agent behavior into a <span>controlled backend workflow.</span>
          </h1>

          <p className="description">
            Superwire is a <strong>declarative DSL</strong> for server-side AI orchestration.
            Define workflows in code, use <strong>scoped tools</strong>, enforce
            <strong> typed outputs</strong> with <strong>validation</strong>, and stream results
            with built-in observability and <strong>streaming execution</strong>.
          </p>

          <a className="docs-link" href="https://docs.superwire.dev">
            <svg className="docs-icon" aria-hidden="true" viewBox="0 0 20 22" fill="none">
              <path d="M4.25 2.75H11.8L15.75 6.7V18.25C15.75 18.8023 15.3023 19.25 14.75 19.25H4.25C3.69772 19.25 3.25 18.8023 3.25 18.25V3.75C3.25 3.19772 3.69772 2.75 4.25 2.75Z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
              <path d="M11.5 2.95V6.95H15.5" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
            </svg>
            Read the documentation
            <svg className="docs-arrow" aria-hidden="true" viewBox="0 0 24 16" fill="none">
              <path d="M1.5 8H21.5M15.5 2L21.5 8L15.5 14" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </a>
        </div>

        <div className="playground-perspective" aria-label="Superwire playground preview">
          <div className="playground-shadow playground-shadow-one" />
          <div className="playground-shadow playground-shadow-two" />

          <div className="playground-frame">
            <div className="app-topbar">
              <img src="/logo-horizontal.svg" alt="" />
              <span className="theme-dot" aria-hidden="true">*</span>
            </div>

            <div className="app-tabs">
              <span className="tab-pill active"><i />Launch brief <b>completed</b></span>
              <span className="tab-pill active"><i />Workflow 2 <b>completed</b></span>
              <span className="new-tab">+ Workflow</span>
            </div>

            <div className="app-canvas">
              <div className="app-actions">
                <span className="view-pill active">⌘ Workflow</span>
                <span className="view-pill">{} Variables</span>
                <span className="validity invalid">invalid</span>
                <span className="text-action">↻ Format</span>
                <span className="text-action">Validate</span>
                <span className="run-button">▷ Run workflow</span>
              </div>

              <div className="editor-panel">
                <div className="editor-title">Workflow 2</div>
                <div className="editor-code">
                  {workflowLines.map((line, workflowLineIndex) => (
                    <div className="code-line" key={`${line.text}-${workflowLineIndex}`}>
                      <span className="line-number">{workflowLineIndex + 1}</span>
                      {line.keyword ? <span className="keyword">{line.keyword}</span> : null}
                      <span>{line.text}</span>
                      {line.accent ? <span className="keyword">{line.accent}</span> : null}
                      {line.suffix ? <span>{line.suffix}</span> : null}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
    </main>
  );
}
