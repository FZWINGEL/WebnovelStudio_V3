import { useEffect, useMemo, useState, type SyntheticEvent } from 'react';
import type { CandidateChoice, SelectedDetail, WorkshopCandidate, WorkshopResult } from '../ipc/workshop';
import './candidateBoard.css';

export interface CandidateBoardProps {
  result: WorkshopResult | null;
  choices: CandidateChoice[];
  selectedDetails: SelectedDetail[];
  onDevelop(candidate: WorkshopCandidate): void;
  onSelectDetail(candidate: WorkshopCandidate, text: string): void;
  onChoice(choice: CandidateChoice): void;
  onExplore(action: string, candidate: WorkshopCandidate): void;
  onSteer?(candidate: WorkshopCandidate, instruction: string): void;
  reviewKey?: string;
  disabled?: boolean;
}

const explorationActions = [
  ['concrete', 'Make it concrete'],
  ['consequences', 'Show consequences'],
  ['challenge', 'Challenge it'],
  ['ordinaryLife', 'Add ordinary life'],
  ['moment', 'Try a moment'],
  ['situation', 'Try a different situation'],
  ['subvert', 'Subvert this direction'],
] as const;

const rationaleOptions = ['Too familiar', 'Wrong mood', 'Breaks a rule'] as const;

function shortContent(content: string, maxWords = 140): { text: string; truncated: boolean } {
  const words = content.trim().split(/\s+/u).filter(Boolean);
  if (words.length <= maxWords) return { text: content.trim(), truncated: false };
  return { text: `${words.slice(0, maxWords).join(' ')}…`, truncated: true };
}

function paragraphs(content: string): string[] {
  return content.split(/\n{2,}/u).map(text => text.trim()).filter(Boolean);
}

function choiceFor(candidateId: string, choices: CandidateChoice[], overrides: Record<string, CandidateChoice>): CandidateChoice | undefined {
  return overrides[candidateId] ?? choices.find(choice => choice.candidateId === candidateId);
}

function runStatusLabel(status: WorkshopResult['run']['status']): string {
  switch (status) {
    case 'queued': return 'Waiting to generate';
    case 'running': return 'Generating';
    case 'stopping': return 'Stopping with useful text retained';
    case 'stopped': return 'Generation stopped';
    case 'failed': return 'Generation failed';
    case 'interrupted': return 'Generation interrupted';
    case 'completed': return 'Generation complete';
  }
}

/**
 * A review surface for bounded story directions. Expansion and local editing
 * are deliberately inert: only the explicit callbacks can affect the story.
 */
export function CandidateBoard({
  result,
  choices,
  selectedDetails,
  onDevelop,
  onSelectDetail,
  onChoice,
  onExplore,
  onSteer,
  reviewKey = '',
  disabled = false,
}: CandidateBoardProps) {
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [detailOpen, setDetailOpen] = useState<Set<string>>(() => new Set());
  const [reviewed, setReviewed] = useState<Set<string>>(() => new Set());
  const [showRecovered, setShowRecovered] = useState(false);
  const [showMore, setShowMore] = useState(false);
  const [rationales, setRationales] = useState<Record<string, string>>({});
  const [selectionRanges, setSelectionRanges] = useState<Record<string, { start: number; end: number }>>({});
  const [choiceOverrides, setChoiceOverrides] = useState<Record<string, CandidateChoice>>({});

  // A new run must not inherit an explicit stale-review approval or a text
  // selection from an earlier result.
  const runKey = result?.run.id ?? null;
  useEffect(() => {
    setExpanded(new Set());
    setDetailOpen(new Set());
    setReviewed(new Set());
    setShowMore(false);
    setSelectionRanges({});
    setChoiceOverrides({});
  }, [runKey]);
  useEffect(() => { setReviewed(new Set()); }, [reviewKey]);

  const output = result?.output;
  const candidateCards = useMemo(() => {
    if (!output) return [];
    return output.candidates.filter(candidate => {
      const choice = choiceFor(candidate.id, choices, choiceOverrides);
      return choice?.status !== 'rejected' && choice?.status !== 'archived';
    });
  }, [choices, choiceOverrides, output]);
  const recoveredCandidates = useMemo(() => {
    if (!output) return [];
    return output.candidates.filter(candidate => {
      const choice = choiceFor(candidate.id, choices, choiceOverrides);
      return choice?.status === 'rejected' || choice?.status === 'archived';
    });
  }, [choices, choiceOverrides, output]);

  function rememberSelection(candidate: WorkshopCandidate, event: SyntheticEvent<HTMLTextAreaElement>) {
    const input = event.currentTarget;
    if (input.selectionStart === input.selectionEnd) {
      setSelectionRanges(previous => { const next = { ...previous }; delete next[candidate.id]; return next; });
      return;
    }
    setSelectionRanges(previous => ({ ...previous, [candidate.id]: { start: input.selectionStart, end: input.selectionEnd } }));
  }

  function emitChoice(candidate: WorkshopCandidate, status: CandidateChoice['status'], includeInContext = false) {
    const existing = choiceFor(candidate.id, choices, choiceOverrides);
    const choice: CandidateChoice = {
      candidateId: candidate.id,
      status,
      rationale: rationales[candidate.id]?.trim() ?? existing?.rationale ?? '',
      // A rejection or archive can never enter a future context packet.
      includeInContext: status === 'saved' ? includeInContext : false,
    };
    setChoiceOverrides(previous => ({ ...previous, [candidate.id]: choice }));
    onChoice(choice);
  }

  function saveCandidate(candidate: WorkshopCandidate) {
    const existing = choiceFor(candidate.id, choices, choiceOverrides);
    emitChoice(candidate, 'saved', existing?.status === 'saved' ? existing.includeInContext : false);
  }

  function developCandidate(candidate: WorkshopCandidate) {
    if (disabled) return;
    if (result?.stale && !reviewed.has(candidate.id)) {
      setReviewed(previous => new Set(previous).add(candidate.id));
      return;
    }
    onDevelop(candidate);
  }

  function selectExact(candidate: WorkshopCandidate, text: string) {
    const exact = text;
    if (!exact) return;
    onSelectDetail(candidate, exact);
  }

  function selectRange(candidate: WorkshopCandidate) {
    const range = selectionRanges[candidate.id];
    if (!range) return;
    selectExact(candidate, candidate.content.slice(range.start, range.end));
  }

  function renderIncomplete() {
    if (!result) return null;
    const raw = result.run.outputText.trim();
    const active = ['queued', 'running', 'stopping'].includes(result.run.status);
    return <section className="candidate-board candidate-board-incomplete" aria-label="Incomplete workshop result">
      <div className="candidate-board-heading">
        <div><h2>{active ? 'Exploration in progress' : 'Generation needs review'}</h2></div>
        <span className="candidate-status candidate-status-incomplete">{runStatusLabel(result.run.status)}</span>
      </div>
      <p className="candidate-board-intro">{active ? 'You can keep developing your working version. Directions will appear here when the response finishes and passes validation.' : 'This response is saved as incomplete text. Review or copy useful passages; it is not a completed proposal.'}</p>
      {result.validationError && <p className="candidate-board-error" role="alert">{result.validationError}</p>}
      {raw ? <pre className="candidate-raw-output">{raw}</pre> : !active && <p className="candidate-board-empty">No usable text was returned.</p>}
    </section>;
  }

  if (!result) return <section className="candidate-board candidate-board-empty-state" aria-label="Candidate comparison"><h2>Compare possible directions</h2><p>Explore your idea to compare a few different directions here. Opening, expanding, and saving cards never makes a story decision.</p></section>;
  if (result.run.status !== 'completed' || !output || !!result.validationError) return renderIncomplete();

  const visibleCandidates = showMore ? candidateCards : candidateCards.slice(0, 3);
  const hiddenCount = Math.max(0, candidateCards.length - 3);
  const moment = result.action === 'moment';
  const voiceGuidance = result.action === 'voiceGuidance';

  return <section className="candidate-board" aria-label="Candidate comparison">
    <header className="candidate-board-heading">
      <div>

        <h2>{voiceGuidance ? 'Compare voice guidance' : 'Compare directions'}</h2>
      </div>
      <div className="candidate-board-status" aria-live="polite">
        <span className="candidate-status">{runStatusLabel(result.run.status)}</span>
        {result.stale && <span className="candidate-status candidate-status-stale">Stale result</span>}
        {moment && <span className="candidate-status candidate-status-noncanon">Noncanon experiment</span>}
        {voiceGuidance && <span className="candidate-status">Proposed style instructions</span>}
      </div>
    </header>

    {(result.stale || moment) && <div className="candidate-board-notice" role="status">
      {result.stale ? 'This result used earlier context or a different working version. Review each direction against current work before developing it; changed sources or relationship scope require a fresh proposal before adoption.' : 'This is a feel test only. Its events stay outside the story until you explicitly adopt them.'}
    </div>}

    <div className="candidate-dimension" aria-label="Comparison dimension"><span>Comparing on</span><strong>{output.dimension}</strong><span>{output.interpretation.possibleDirection}</span></div>

    <div className="candidate-grid">
      {visibleCandidates.map(candidate => <CandidateCard
        key={candidate.id}
        candidate={candidate}
        outputDimension={output.dimension}
        choice={choiceFor(candidate.id, choices, choiceOverrides)}
        expanded={expanded.has(candidate.id)}
        detailsOpen={detailOpen.has(candidate.id)}
        reviewed={reviewed.has(candidate.id)}
        rationale={rationales[candidate.id] ?? choiceFor(candidate.id, choices, choiceOverrides)?.rationale ?? ''}
        range={selectionRanges[candidate.id]}
        selectedDetails={selectedDetails}
        stale={result.stale}
        disabled={disabled}
        onToggleExpanded={() => setExpanded(previous => { const next = new Set(previous); next.has(candidate.id) ? next.delete(candidate.id) : next.add(candidate.id); return next; })}
        onToggleDetails={() => setDetailOpen(previous => { const next = new Set(previous); next.has(candidate.id) ? next.delete(candidate.id) : next.add(candidate.id); return next; })}
        onReview={() => setReviewed(previous => new Set(previous).add(candidate.id))}
        onDevelop={() => developCandidate(candidate)}
        onSave={() => saveCandidate(candidate)}
        onReject={() => emitChoice(candidate, 'rejected')}
        onArchive={() => emitChoice(candidate, 'archived')}
        onRestore={() => emitChoice(candidate, 'saved')}
        onRationaleChange={value => setRationales(previous => ({ ...previous, [candidate.id]: value }))}
        onIncludeChange={include => {
          const current = choiceFor(candidate.id, choices, choiceOverrides);
          if (current?.status === 'saved') emitChoice(candidate, 'saved', include);
        }}
        onRememberSelection={event => rememberSelection(candidate, event)}
        onSelectRange={() => selectRange(candidate)}
        onSelectFull={() => selectExact(candidate, candidate.content)}
        onSelectParagraph={text => selectExact(candidate, text)}
        onExplore={action => onExplore(action, candidate)}
        onSteer={onSteer ? instruction => onSteer(candidate, instruction) : undefined}
      />)}
    </div>

    {hiddenCount > 0 && <button type="button" className="candidate-board-more" disabled={disabled} onClick={() => setShowMore(value => !value)}>{showMore ? 'Show three directions' : `Show ${hiddenCount} more direction${hiddenCount === 1 ? '' : 's'}`}</button>}

    {recoveredCandidates.length > 0 && <section className="candidate-recovered" aria-label="Recovered alternatives">
      <label className="candidate-recovered-toggle"><input type="checkbox" checked={showRecovered} onChange={event => setShowRecovered(event.target.checked)} />Show saved or rejected alternatives</label>
      {showRecovered && <div className="candidate-recovered-list">{recoveredCandidates.map(candidate => {
        const choice = choiceFor(candidate.id, choices, choiceOverrides)!;
        return <article className="candidate-recovered-item" key={candidate.id}>
          <div><strong>{candidate.title}</strong><span className="candidate-choice-state">{choice.status}</span></div>
          <p>{choice.status === 'rejected' ? 'Rejected alternatives stay out of future context.' : 'Archived alternatives remain recoverable in this session.'}</p>
          <button type="button" className="secondary-button" disabled={disabled} onClick={() => emitChoice(candidate, 'saved')}>Restore for exploration</button>
        </article>;
      })}</div>}
    </section>}

  </section>;
}

interface CandidateCardProps {
  candidate: WorkshopCandidate;
  outputDimension: string;
  choice?: CandidateChoice;
  expanded: boolean;
  detailsOpen: boolean;
  reviewed: boolean;
  rationale: string;
  range?: { start: number; end: number };
  selectedDetails: SelectedDetail[];
  stale: boolean;
  disabled: boolean;
  onToggleExpanded(): void;
  onToggleDetails(): void;
  onReview(): void;
  onDevelop(): void;
  onSave(): void;
  onReject(): void;
  onArchive(): void;
  onRestore(): void;
  onRationaleChange(value: string): void;
  onIncludeChange(value: boolean): void;
  onRememberSelection(event: SyntheticEvent<HTMLTextAreaElement>): void;
  onSelectRange(): void;
  onSelectFull(): void;
  onSelectParagraph(text: string): void;
  onExplore(action: string): void;
  onSteer?(instruction: string): void;
}

function CandidateCard({
  candidate, outputDimension, choice, expanded, detailsOpen, reviewed, rationale, range,
  selectedDetails, stale, disabled, onToggleExpanded, onToggleDetails, onReview, onDevelop,
  onSave, onReject, onArchive, onRestore, onRationaleChange, onIncludeChange,
  onRememberSelection, onSelectRange, onSelectFull, onSelectParagraph, onExplore, onSteer,
}: CandidateCardProps) {
  const preview = shortContent(candidate.content);
  const selectedText = range ? candidate.content.slice(range.start, range.end) : '';
  const status = choice?.status;
  const detailList = paragraphs(candidate.content);
  return <article className={`candidate-card${status ? ` candidate-card-${status}` : ''}`} tabIndex={0} aria-labelledby={`candidate-title-${candidate.id}`}>
    <div className="candidate-card-heading">
      <div><h3 id={`candidate-title-${candidate.id}`}>{candidate.title}</h3></div>
      {status && <span className="candidate-choice-state">{status}</span>}
    </div>
    <dl className="candidate-card-dimension"><div><dt>{outputDimension}</dt><dd>{candidate.dimensionValue}</dd></div></dl>
    <p className="candidate-content">{expanded || !preview.truncated ? candidate.content : preview.text}</p>
    {preview.truncated && <button type="button" className="text-button candidate-expand" aria-expanded={expanded} onClick={onToggleExpanded}>{expanded ? 'Show less' : 'Read the full direction'}</button>}
    {expanded && !detailsOpen && <CandidateEvidence candidate={candidate} compact />}

    <div className="candidate-card-actions">
      <button type="button" className="primary-button" disabled={disabled} onClick={onDevelop}>{stale && !reviewed ? 'Review against current work' : 'Develop this'}</button>
      <button type="button" className="secondary-button" disabled={disabled} aria-expanded={detailsOpen} onClick={onToggleDetails}>Select details</button>
      <button type="button" className="secondary-button" disabled={disabled} onClick={onSave}>Save for later</button>
    </div>

    {status === 'saved' && <label className="candidate-include"><input type="checkbox" checked={!!choice?.includeInContext} disabled={disabled} onChange={event => onIncludeChange(event.target.checked)} />Include this saved direction in the next exploration</label>}

    <details className="candidate-rationale"><summary>Add a reason for this choice</summary>
      <label htmlFor={`candidate-rationale-${candidate.id}`}>Optional reason for saving or rejecting</label>
      <textarea id={`candidate-rationale-${candidate.id}`} rows={2} maxLength={1000} value={rationale} disabled={disabled} onChange={event => onRationaleChange(event.target.value)} placeholder="Keep this local to the decision…" />
      <div className="candidate-rationale-chips" aria-label="Rationale suggestions">{rationaleOptions.map(option => <button type="button" key={option} className={rationale === option ? 'active' : ''} disabled={disabled} aria-pressed={rationale === option} onClick={() => onRationaleChange(option)}>{option}</button>)}</div>
    </details>

    <details className="candidate-more-actions"><summary>More actions</summary><div className="candidate-more-actions-list"><button type="button" disabled={disabled} onClick={onReject}>Reject</button><button type="button" disabled={disabled} onClick={onArchive}>Archive</button>{status && status !== 'saved' && <button type="button" disabled={disabled} onClick={onRestore}>Restore for exploration</button>}</div></details>

    {detailsOpen && <section className="candidate-details" aria-label={`${candidate.title} details`}>
      <h4>Select exact details</h4>
      <p className="candidate-detail-note">Choose a whole paragraph or select an exact substring in the text area. The selected text is forwarded exactly as shown; expanding this section makes no model request.</p>
      <CandidateEvidence candidate={candidate} />
      {onSteer && !!candidate.assumptions.length && <div className="candidate-assumption-actions"><h4>Decide which assumptions to explore</h4>{candidate.assumptions.map(assumption => <div key={assumption}><p>{assumption}</p><button disabled={disabled} onClick={() => onSelectParagraph(assumption)}>Keep this assumption in my tray</button><button disabled={disabled} onClick={() => onSteer(`Reject this assumption: ${assumption}. Explore a contrasting implication without relying on it.`)}>Prepare a contrasting implication</button></div>)}</div>}
      <textarea className="candidate-exact-text" aria-label={`Exact text from ${candidate.title}`} readOnly value={candidate.content} onSelect={onRememberSelection} onKeyUp={onRememberSelection} onMouseUp={onRememberSelection} />
      <div className="candidate-select-actions"><button type="button" className="secondary-button" disabled={disabled} onClick={onSelectFull}>Select full direction</button><button type="button" className="secondary-button" disabled={disabled || !selectedText} onClick={onSelectRange}>Select selected text</button></div>
      {selectedText && <p className="candidate-selection-preview">Ready to select exactly: “{selectedText}”</p>}
      {detailList.length > 1 && <div className="candidate-paragraph-actions"><span>Select a paragraph:</span>{detailList.map((text, index) => <button type="button" key={`${candidate.id}-paragraph-${index}`} className={selectedDetails.some(detail => detail.candidateId === candidate.id && detail.text === text) ? 'active' : ''} disabled={disabled} onClick={() => onSelectParagraph(text)}>{index + 1}</button>)}</div>}
      <details className="candidate-explore-menu"><summary>Explore another angle</summary><div className="candidate-explore-actions">{explorationActions.map(([action, label]) => <button type="button" key={action} disabled={disabled} onClick={() => onExplore(action)}>{label}</button>)}</div></details>
    </section>}
  </article>;
}

function CandidateEvidence({ candidate, compact = false }: { candidate: WorkshopCandidate; compact?: boolean }) {
  return <div className={`candidate-evidence${compact ? ' candidate-evidence-compact' : ''}`}>
    {!compact && <div className="candidate-preserved-changed"><div><h5>Preserved</h5>{candidate.preservedDetails.length ? <ul>{candidate.preservedDetails.map(item => <li key={item}>{item}</li>)}</ul> : <p>Nothing recorded.</p>}</div><div><h5>Changed</h5>{candidate.changedDetails.length ? <ul>{candidate.changedDetails.map(item => <li key={item}>{item}</li>)}</ul> : <p>Nothing recorded.</p>}</div></div>}
    <div className="candidate-implications"><h4>Implications, basis, and assumptions</h4>{candidate.implications.length ? <ul>{candidate.implications.map((implication, index) => <li key={`${candidate.id}-implication-${index}`}><strong>{implication.text}</strong><span><b>Based on:</b> {implication.basis}</span><span><b>Assumption:</b> {implication.assumption}</span></li>)}</ul> : <p>No implications were recorded for this direction.</p>}{candidate.assumptions.length > 0 && <><h5>Open assumptions</h5><ul>{candidate.assumptions.map(item => <li key={item}>{item}</li>)}</ul></>}</div>
  </div>;
}
