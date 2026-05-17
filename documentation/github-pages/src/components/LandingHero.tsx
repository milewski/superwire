import { ArrowRight, FileText } from 'lucide-react';
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
    'M2 126 H86 C114 126 114 158 143 158 H194',
    'M0 262 H136 C164 262 164 296 193 296 H238',
    'M102 24 V88 C102 112 82 116 63 116 H0',
    'M682 58 H755 C783 58 786 92 814 92 H878',
    'M710 300 H807 C835 300 835 334 864 334 H930',
    'M686 444 H760 C790 444 790 492 820 492 H932',
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

      {circuitPaths.map((circuitPath, circuitPathIndex) => (
        <motion.path
          key={circuitPath}
          d={circuitPath}
          fill="none"
          stroke="#ff7900"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.25"
          filter="url(#circuit-glow)"
          initial={{ pathLength: 0, opacity: 0 }}
          animate={{ pathLength: [0, 1, 1], opacity: [0, 0.88, 0.45] }}
          transition={{
            delay: 0.7 + circuitPathIndex * 0.16,
            duration: 2.8,
            ease: 'easeInOut',
            repeat: Infinity,
            repeatDelay: 1.2,
          }}
        />
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
        <div className="editor-topbar">
          <div className="editor-brand">superwire</div>
          <div className="editor-star">*</div>
        </div>

        <div className="editor-tabs-row">
          <div className="editor-pill"><span />Launch brief <strong>COMPLETED</strong></div>
          <div className="editor-pill editor-pill-active"><span />Workflow 2 <strong>COMPLETED</strong></div>
          <button className="editor-add-button" type="button">+ Workflow</button>
        </div>

        <div className="editor-toolbar">
          <div className="editor-toggle editor-toggle-active">Workflow</div>
          <div className="editor-toggle">&#123;&#125; Variables</div>
          <div className="editor-toolbar-spacer" />
          <div className="editor-invalid">INVALID</div>
          <div className="editor-action">Format</div>
          <div className="editor-action">Validate</div>
          <button className="editor-run-button" type="button">&#9655; Run workflow</button>
        </div>

        <div className="editor-code-card">
          <div className="editor-code-title">Workflow 2</div>
          <div className="editor-code" aria-label="Superwire workflow code preview">
            {codeLines.map((codeLine, codeLineIndex) => (
              <div className="editor-code-line" key={`code-line-${codeLineIndex + 1}`}>
                <span className="editor-line-number">{codeLineIndex + 1}</span>
                <span className="editor-line-content">
                  {codeLine.map((codeSegment, codeSegmentIndex) => {
                    const colorName = codeSegment.color;
                    const className = colorName ? colorClassNames[colorName] : 'text-[#d6d6d6]';

                    return <span className={className} key={`${codeSegment.text}-${codeSegmentIndex}`}>{codeSegment.text}</span>;
                  })}
                </span>
              </div>
            ))}
          </div>
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
              <ArrowRight aria-hidden="true" size={27} strokeWidth={2.2} />
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
