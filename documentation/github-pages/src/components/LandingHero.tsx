import { ArrowRight, Braces, Copy, FileText, Pencil, Play, Plus, RefreshCcw, Sun, Trash2, Workflow } from 'lucide-react';
import { motion } from 'motion/react';
import logoUrl from '../../../docs/public/logo-horizontal.svg';

const documentationUrl = 'https://docs.superwire.dev';

type CodeSegmentColor = 'keyword' | 'number' | 'plain' | 'property' | 'reference' | 'string' | 'type';

type CodeSegment = {
  text: string;
  color?: CodeSegmentColor;
};

const codeLines: CodeSegment[][] = [
  [{ text: 'provider', color: 'keyword' }, { text: ' openai ', color: 'plain' }, { text: 'from', color: 'keyword' }, { text: ' openai {' }],
  [{ text: '  endpoint: ', color: 'property' }, { text: '"http://100.118.299.48:3000/v1"', color: 'string' }],
  [{ text: '  api_key: ', color: 'property' }, { text: '"sk-CLKR4I0qU4oPFyTNjACCTDrqO66EMYTx1PNFSoolZF6wFuzz"', color: 'string' }],
  [{ text: '}' }],
  [],
  [{ text: 'model', color: 'keyword' }, { text: ' openai_model ', color: 'plain' }, { text: 'from', color: 'keyword' }, { text: ' openai {' }],
  [{ text: '  id: ', color: 'property' }, { text: '"big-pickle"', color: 'string' }],
  [{ text: '}' }],
  [],
  [{ text: 'mcp', color: 'keyword' }, { text: ' local {' }],
  [{ text: '  endpoint: ', color: 'property' }, { text: '"http://localhost:8000/mcp/summarizer"', color: 'string' }],
  [{ text: '  headers {' }],
  [{ text: '    Accept: ', color: 'property' }, { text: '"application/json"', color: 'string' }],
  [{ text: '    Authorization: ', color: 'property' }, { text: '"Bearer 78N!CJXMMCJrHwFa6qApHt7X8Pg00NiLj1MKXyR81da8Sdce"', color: 'string' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'from', color: 'keyword' }, { text: ' mcp.local {' }],
  [{ text: '  bindings {' }],
  [{ text: '    project_id: ', color: 'property' }, { text: '14', color: 'number' }],
  [{ text: '    task_id: ', color: 'property' }, { text: '109', color: 'number' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'prompt', color: 'keyword' }, { text: ' dynamic_summary_prompt {' }],
  [{ text: '  bindings {' }],
  [{ text: '    project_id: ', color: 'property' }, { text: '14', color: 'number' }],
  [{ text: '    type: ', color: 'property' }, { text: '"task"', color: 'string' }],
  [{ text: '    type_id: ', color: 'property' }, { text: '109', color: 'number' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'tool', color: 'keyword' }, { text: ' list_all_participants_who_has_answered_given_task' }],
  [{ text: 'tool', color: 'keyword' }, { text: ' fetch_participant_answer' }],
  [],
  [{ text: 'agent', color: 'keyword' }, { text: ' greeting {' }],
  [{ text: '  model: ', color: 'property' }, { text: 'model.openai_model', color: 'reference' }],
  [{ text: '  uses: [', color: 'property' }, { text: 'tool', color: 'keyword' }, { text: '.list_all_participants_who_has_answered_given_task, ', color: 'plain' }, { text: 'prompt', color: 'keyword' }, { text: '.dynamic_summary_prompt]' }],
  [],
  [{ text: '  instruction: ', color: 'property' }, { text: '"""', color: 'string' }],
  [{ text: '    call the ', color: 'string' }, { text: 'prompt', color: 'keyword' }, { text: ' to figure out extra instructions user my request', color: 'string' }],
  [{ text: '    Please analyze all tasks of the participants and provide me a summary', color: 'string' }],
  [{ text: '  """', color: 'string' }],
  [],
  [{ text: '  output', color: 'keyword' }, { text: ' {' }],
  [{ text: '    summary: ', color: 'property' }, { text: 'string', color: 'type' }],
  [{ text: '  }' }],
  [{ text: '}' }],
  [],
  [{ text: 'output', color: 'keyword' }, { text: ' {' }],
  [{ text: '  greeting: ', color: 'property' }, { text: 'agent', color: 'keyword' }, { text: '.greeting.summary' }],
  [{ text: '}' }],
];

const colorClassNames = {
  keyword: 'text-[#ff7b00]',
  number: 'text-[#ffd28b]',
  plain: 'text-[#d7d7d7]',
  property: 'text-[#7bb7ff]',
  reference: 'text-[#8ce6b0]',
  string: 'text-[#94e5b6]',
  type: 'text-[#a8c7ff]',
};

function CircuitLines() {
  const circuitPaths = [
    { path: 'M2 126 H86 C114 126 114 158 143 158 H194', duration: 5.2, delay: 0 },
    { path: 'M0 262 H136 C164 262 164 296 193 296 H238', duration: 6.4, delay: 0.35 },
    { path: 'M102 24 V88 C102 112 82 116 63 116 H0', duration: 5.8, delay: 0.7 },
    { path: 'M682 58 H755 C783 58 786 92 814 92 H878', duration: 6.1, delay: 0.1 },
    { path: 'M710 300 H807 C835 300 835 334 864 334 H930', duration: 5.6, delay: 0.55 },
    { path: 'M686 444 H760 C790 444 790 492 820 492 H932', duration: 6.8, delay: 0.85 },
  ];

  return (
    <svg aria-hidden="true" className="circuit-board" viewBox="0 0 930 560" preserveAspectRatio="none">
      <defs>
        <filter id="circuit-glow" x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="2.4" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      {circuitPaths.map((circuitPath) => (
        <g key={circuitPath.path}>
          <path
            d={circuitPath.path}
            fill="none"
            stroke="#ff7900"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1"
            opacity="0.34"
          />
          <motion.path
            d={circuitPath.path}
            fill="none"
            stroke="#ff8a14"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1.35"
            strokeDasharray="1 17"
            filter="url(#circuit-glow)"
            initial={{ strokeDashoffset: 0, opacity: 0.32 }}
            animate={{ strokeDashoffset: -72, opacity: [0.32, 0.95, 0.32] }}
            transition={{
              strokeDashoffset: {
                delay: circuitPath.delay,
                duration: circuitPath.duration,
                ease: 'linear',
                repeat: Infinity,
              },
              opacity: {
                delay: circuitPath.delay,
                duration: circuitPath.duration * 0.5,
                ease: 'easeInOut',
                repeat: Infinity,
                repeatType: 'mirror',
              },
            }}
          />
        </g>
      ))}

      <motion.circle
        cx="2"
        cy="126"
        r="5"
        fill="#ff7900"
        animate={{ opacity: [0.35, 1, 0.35], scale: [0.92, 1.18, 0.92] }}
        transition={{ duration: 2.1, repeat: Infinity, ease: 'easeInOut' }}
      />
      <motion.circle
        cx="0"
        cy="262"
        r="4.5"
        fill="#ff7900"
        animate={{ opacity: [0.22, 0.92, 0.22], scale: [0.9, 1.2, 0.9] }}
        transition={{ delay: 0.8, duration: 2.4, repeat: Infinity, ease: 'easeInOut' }}
      />
    </svg>
  );
}

function EditorWindow() {
  return (
    <motion.div
      className="editor-perspective"
      initial={{ opacity: 0, rotateX: 10, rotateY: -18, rotateZ: 2, scale: 0.88, y: 72 }}
      animate={{ opacity: 1, rotateX: 2, rotateY: -7, rotateZ: 1.2, scale: 1, y: 0 }}
      transition={{ duration: 1.25, ease: [0.16, 1, 0.3, 1], delay: 0.28 }}
    >
      <div className="editor-extrusion" />
      <div className="editor-panel">
        <div className="playground-preview dark">
          <section className="playground__frame">
            <div className="playground__main">
              <header className="playground__topbar">
                <div className="playground__brand">
                  <img src={logoUrl.src} alt="Superwire" className="playground__logo" />
                </div>

                <div className="playground__topbar-actions">
                  <button className="button button--ghost button--icon-lg playground__theme-toggle" type="button" aria-label="Toggle theme">
                    <Sun />
                  </button>
                </div>
              </header>

              <div className="playground__tabs">
                <div className="tabs-list">
                  <div className="playground-tabs__item">
                    <button className="playground-tabs__trigger" type="button" data-state="inactive">
                      <span className="playground-tabs__dot" />
                      <span className="playground-tabs__title">Launch brief</span>
                      <span className="mini-status completed">completed</span>
                    </button>

                    <div className="playground-tabs__actions">
                      <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Rename Launch brief"><Pencil /></button>
                      <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Duplicate Launch brief"><Copy /></button>
                      <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Close Launch brief"><Trash2 /></button>
                    </div>
                  </div>

                  <div className="playground-tabs__item playground-tabs__item--active">
                    <button className="playground-tabs__trigger" type="button" data-state="active" data-active>
                      <span className="playground-tabs__dot" />
                      <span className="playground-tabs__title">Workflow 2</span>
                      <span className="mini-status completed">completed</span>
                    </button>

                    <div className="playground-tabs__actions">
                      <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Rename Workflow 2"><Pencil /></button>
                      <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Duplicate Workflow 2"><Copy /></button>
                      <button className="button button--ghost button--icon-sm playground-tabs__action" type="button" aria-label="Close Workflow 2"><Trash2 /></button>
                    </div>
                  </div>

                  <button className="button button--outline button--lg playground-tabs__new" type="button"><Plus /> Workflow</button>
                </div>
              </div>

              <div className="playground__canvas">
                <section className="playground__content">
                  <div className="playground__controls">
                    <nav className="playground-mode-switch" aria-label="Playground mode">
                      <button className="button button--secondary button--lg playground-mode-switch__button" type="button"><Workflow /> Workflow</button>
                      <button className="button button--ghost button--lg playground-mode-switch__button" type="button"><Braces /> Variables</button>
                    </nav>

                    <div className="playground-actions">
                      <span className="status-pill invalid">invalid</span>
                      <button className="button button--ghost button--lg" type="button"><RefreshCcw /> Format</button>
                      <button className="button button--ghost button--lg" type="button">Validate</button>
                      <button className="button button--lg playground-actions__run" type="button"><Play /> Run workflow</button>
                    </div>
                  </div>

                  <section className="workflow-layout">
                    <div className="workflow-layout__top workflow-layout__top--single">
                      <article className="workflow-editor">
                        <div className="workflow-editor__header panel-card__header">
                          <div className="panel-card__title-block">
                            <strong>Workflow 2</strong>
                          </div>
                        </div>

                        <div className="wire-editor-shell">
                          <div className="wire-editor-preview" aria-label="Superwire workflow code preview">
                            <div className="cm-gutters" aria-hidden="true">
                              {codeLines.map((_, codeLineIndex) => <span key={`gutter-${codeLineIndex + 1}`}>{codeLineIndex + 1}</span>)}
                            </div>

                            <div className="cm-content">
                              {codeLines.map((codeLine, codeLineIndex) => (
                                <div className="cm-line" key={`code-line-${codeLineIndex + 1}`}>
                                  {codeLine.map((codeSegment, codeSegmentIndex) => {
                                    const colorName = codeSegment.color;
                                    const className = colorName ? colorClassNames[colorName] : 'text-[#d6d6d6]';

                                    return <span className={className} key={`${codeSegment.text}-${codeSegmentIndex}`}>{codeSegment.text}</span>;
                                  })}
                                </div>
                              ))}
                            </div>
                          </div>
                        </div>

                        <div className="workflow-editor__message workflow-editor__message--error">
                          <span className="workflow-editor__message-line workflow-editor__message-line--full">Unable to validate workflow: provider endpoint is not reachable.</span>
                        </div>
                      </article>
                    </div>

                    <div className="workflow-layout__bottom">
                      <article className="panel-card workflow-log-panel" data-state="open">
                        <div className="panel-card__header">
                          <div className="panel-card__title-block">
                            <strong>Output</strong>
                            <small>Final workflow output payload.</small>
                          </div>
                        </div>
                        <div className="workflow-log-panel__body">
                          <pre className="workflow-output__json">{"{\n  \"greeting\": \"Summary is ready.\"\n}"}</pre>
                        </div>
                      </article>

                      <article className="panel-card workflow-log-panel" data-state="open">
                        <div className="panel-card__header">
                          <div className="panel-card__title-block">
                            <strong>Server events</strong>
                            <small>3 streamed events.</small>
                          </div>
                        </div>
                        <div className="workflow-log-panel__body events-log">
                          <div className="events-log__item">
                            <div className="events-log__item-trigger">
                              <span className="events-log__item-meta"><span className="event-chip event-completed">completed</span><span className="events-log__item-summary">agent.greeting finished</span></span>
                              <span className="events-log__item-time">12ms</span>
                            </div>
                          </div>
                        </div>
                      </article>
                    </div>
                  </section>
                </section>
              </div>
            </div>
          </section>
          </div>
        </div>
    </motion.div>
  );
}

export default function LandingHero() {
  return (
    <main className="hero-shell">
      <div className="hero-noise" />
      <div className="hero-grid" />
      <div className="hero-inner">
        <motion.section
          className="hero-copy"
          initial={{ opacity: 0, x: -42, filter: 'blur(10px)' }}
          animate={{ opacity: 1, x: 0, filter: 'blur(0px)' }}
          transition={{ duration: 0.9, ease: [0.16, 1, 0.3, 1] }}
        >
          <img className="hero-logo" src={logoUrl.src} alt="Superwire" />

          <div className="hero-copy-content">
            <motion.h1
              initial={{ opacity: 0, y: 28 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.9, delay: 0.16, ease: [0.16, 1, 0.3, 1] }}
            >
              Turn AI agent behavior into a <span>controlled backend workflow.</span>
            </motion.h1>

            <motion.p
              initial={{ opacity: 0, y: 24 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.85, delay: 0.32, ease: [0.16, 1, 0.3, 1] }}
            >
              Superwire is a <strong>declarative DSL</strong> for server-side AI orchestration. Define workflows in code,
              use <strong>scoped tools</strong>, enforce <strong>typed outputs</strong> with <strong>validation</strong>, and
              stream results with built-in observability and <strong>streaming execution</strong>.
            </motion.p>

            <motion.a
              className="documentation-button"
              href={documentationUrl}
              initial={{ opacity: 0, y: 22 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.78, delay: 0.48, ease: [0.16, 1, 0.3, 1] }}
            >
              <FileText aria-hidden="true" size={25} strokeWidth={2.2} />
              <span>Read the documentation</span>
              <ArrowRight className="documentation-button__arrow" aria-hidden="true" size={27} strokeWidth={2.2} />
            </motion.a>
          </div>
        </motion.section>

        <section className="hero-visual" aria-label="Superwire editor preview">
          <CircuitLines />
          <EditorWindow />
        </section>
      </div>
    </main>
  );
}
