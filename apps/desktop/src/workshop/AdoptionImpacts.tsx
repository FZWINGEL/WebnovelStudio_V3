import type { DocumentRecord } from '../ipc/projects';
import type { WorkshopImpact, WorkshopImpactDraft } from '../ipc/workshop';
import { plainText } from './text';

export const IMPACT_LABELS: Record<WorkshopImpact['kind'], string> = {
  contradiction: 'Clear contradiction', possibleTension: 'Possible tension',
  dependentAssumption: 'Dependent assumption', styleSuggestion: 'Style suggestion',
};

export function AdoptionImpacts({ impacts, documents, onChange }: {
  impacts: WorkshopImpactDraft[]; documents: DocumentRecord[]; onChange(impacts: WorkshopImpactDraft[]): void;
}) {
  if (!impacts.length) return null;
  return <section aria-label="Review affected material"><h3>Material that may need a later review</h3>
    <p className="small-copy">These are connections suggested by the selected directions. Review their reasons and classification. Adoption records a review flag; it does not repair or rewrite this material.</p>
    {impacts.map((impact, index) => {
      const source = documents.find(document => document.head.documentId === impact.documentId);
      const edit = (change: Partial<WorkshopImpactDraft>) => onChange(impacts.map((item, at) => at === index ? { ...item, ...change } : item));
      return <fieldset key={impact.documentId}><legend>{source?.title ?? 'Unavailable source'}</legend>
        <label>Type of effect<select value={impact.kind} onChange={event => edit({ kind: event.target.value as WorkshopImpact['kind'] })}>{Object.entries(IMPACT_LABELS).map(([kind, label]) => <option key={kind} value={kind}>{label}</option>)}</select></label>
        <label>Reason to review<textarea value={impact.reason} maxLength={6000} onChange={event => edit({ reason: event.target.value })} /></label>
        {source && <details><summary>Current source · version {source.head.version}</summary><p className="workshop-prose">{plainText(source.body)}</p></details>}
      </fieldset>;
    })}
  </section>;
}
