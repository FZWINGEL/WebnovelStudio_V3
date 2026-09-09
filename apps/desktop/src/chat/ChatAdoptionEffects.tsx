import type { ReactNode } from 'react';
import type { ChatAdoptionEffects as EffectsManifest, ChatAdoptionTarget } from '../ipc/projectChat';

export interface ChatAdoptionEffectsProps {
  effects?: EffectsManifest | null;
  targets?: ChatAdoptionTarget[];
}

function targetLabel(documentId: string, targets: ChatAdoptionTarget[]): string {
  const target = targets.find(candidate => candidate.documentId === documentId);
  return target ? `${target.title} · ${target.kind}` : 'Related document';
}

function VersionDetails({ children }: { children: ReactNode }) {
  return <details className="chat-adoption-version-details">
    <summary>Version details</summary>
    <div>{children}</div>
  </details>;
}

function EmptyList({ children }: { children: string }) {
  return <p className="chat-muted">{children}</p>;
}

export function ChatAdoptionEffects({ effects, targets = [] }: ChatAdoptionEffectsProps) {
  if (effects === undefined) {
    return <section className="chat-adoption-effects" aria-label="Adoption effects manifest">
      <h4>Effects manifest</h4>
      <p className="chat-draft-warning">This historical preview does not include an effects manifest. Prepare a new preview to inspect current dependency and effect evidence.</p>
    </section>;
  }
  if (effects === null) {
    return <section className="chat-adoption-effects" aria-label="Adoption effects manifest">
      <h4>Effects manifest</h4>
      <p className="chat-draft-warning">No effects manifest is available for this preview.</p>
    </section>;
  }

  const hasDependencies = effects.relationshipDependencies.length > 0;
  const hasProtectedContent = effects.protectedContent.length > 0;
  const hasRelationships = effects.proposedRelationships.length > 0;
  const hasImpacts = effects.impacts.length > 0;
  const hasSupersessions = effects.supersessions.length > 0;
  const hasPlacements = effects.placements.length > 0;
  const hasAnyGroup = hasDependencies || hasProtectedContent || hasRelationships || hasImpacts || hasSupersessions || hasPlacements;

  return <section className="chat-adoption-effects" aria-label="Adoption effects manifest">
    <header>
      <h4>Complete effects manifest</h4>
      <VersionDetails>
        <dl>
          <div><dt>Manifest version</dt><dd>{effects.version}</dd></div>
          <div><dt>Source output hash</dt><dd>{effects.sourceOutputHash}</dd></div>
        </dl>
      </VersionDetails>
    </header>
    <p className="chat-prose">Each target is a whole-document proposal. Existing metadata and order are protected. Placement suggestions are shown for explicit validation and never change order automatically.</p>

    {hasDependencies && <section aria-label="Relationship dependencies">
      <h5>Relationship dependencies ({effects.relationshipDependencies.length})</h5>
      <ul>{effects.relationshipDependencies.map(dependency => <li key={dependency.relationshipId}>
        <strong>{dependency.relationshipType}</strong>
        <span> · {targetLabel(dependency.fromDocumentId, targets)} → {targetLabel(dependency.toDocumentId, targets)}</span>
        <VersionDetails>
          <dl>
            <div><dt>Relationship ID</dt><dd>{dependency.relationshipId}</dd></div>
            <div><dt>From document ID</dt><dd>{dependency.fromDocumentId}</dd></div>
            <div><dt>To document ID</dt><dd>{dependency.toDocumentId}</dd></div>
            <div><dt>From head</dt><dd>{JSON.stringify(dependency.fromHead)}</dd></div>
            <div><dt>To head</dt><dd>{JSON.stringify(dependency.toHead)}</dd></div>
          </dl>
        </VersionDetails>
      </li>)}</ul>
    </section>}

    {hasProtectedContent && <section aria-label="Protected existing content">
      <h5>Protected existing content ({effects.protectedContent.length})</h5>
      <ul>{effects.protectedContent.map((protectedItem, index) => <li key={`${protectedItem.targetDocumentId}:${protectedItem.textHash}:${index}`}>
        <strong>{targetLabel(protectedItem.targetDocumentId, targets)}</strong>
        <p className="chat-prose">{protectedItem.text}</p>
        <VersionDetails>
          <dl>
            <div><dt>Target document ID</dt><dd>{protectedItem.targetDocumentId}</dd></div>
            <div><dt>Source head</dt><dd>{JSON.stringify(protectedItem.sourceHead)}</dd></div>
            <div><dt>Text hash</dt><dd>{protectedItem.textHash}</dd></div>
          </dl>
        </VersionDetails>
      </li>)}</ul>
    </section>}

    {hasRelationships && <section aria-label="Proposed relationships">
      <h5>Proposed relationships ({effects.proposedRelationships.length})</h5>
      <ul>{effects.proposedRelationships.map(relationship => <li key={relationship.key}>
        <strong>{relationship.type}</strong>
        <span> · {targetLabel(relationship.fromDocumentId, targets)} → {targetLabel(relationship.toDocumentId, targets)}</span>
        <p className="chat-prose">{relationship.description}</p>
        <p className="chat-muted">Uncertainty: {relationship.uncertainty || 'Not stated.'}</p>
        <VersionDetails>
          <dl>
            <div><dt>Proposal key</dt><dd>{relationship.key}</dd></div>
            <div><dt>Relationship ID</dt><dd>{relationship.relationshipId}</dd></div>
            <div><dt>From document ID</dt><dd>{relationship.fromDocumentId}</dd></div>
            <div><dt>To document ID</dt><dd>{relationship.toDocumentId}</dd></div>
            <div><dt>From head</dt><dd>{JSON.stringify(relationship.fromHead)}</dd></div>
            <div><dt>To head</dt><dd>{JSON.stringify(relationship.toHead)}</dd></div>
          </dl>
        </VersionDetails>
      </li>)}</ul>
    </section>}

    {hasImpacts && <section aria-label="Proposed impacts">
      <h5>Proposed impacts ({effects.impacts.length})</h5>
      <ul>{effects.impacts.map((impact, index) => <li key={`${impact.targetDocumentId}:${impact.kind}:${index}`}>
        <strong>{targetLabel(impact.targetDocumentId, targets)} · {impact.kind}</strong>
        <p className="chat-prose">{impact.reason}</p>
        <VersionDetails>
          <dl>
            <div><dt>Target document ID</dt><dd>{impact.targetDocumentId}</dd></div>
            {impact.relationshipId && <div><dt>Relationship ID</dt><dd>{impact.relationshipId}</dd></div>}
            {impact.relationshipKey && <div><dt>Relationship proposal key</dt><dd>{impact.relationshipKey}</dd></div>}
          </dl>
        </VersionDetails>
      </li>)}</ul>
    </section>}

    {hasSupersessions && <section aria-label="Proposed supersessions">
      <h5>Proposed supersessions ({effects.supersessions.length})</h5>
      <ul>{effects.supersessions.map((supersession, index) => <li key={`${supersession.targetDocumentId}:${supersession.supersededDocumentId}:${index}`}>
        <strong>{targetLabel(supersession.targetDocumentId, targets)} supersedes {targetLabel(supersession.supersededDocumentId, targets)}</strong>
        <p className="chat-prose">{supersession.reason}</p>
        <VersionDetails>
          <dl>
            <div><dt>Target document ID</dt><dd>{supersession.targetDocumentId}</dd></div>
            <div><dt>Superseded document ID</dt><dd>{supersession.supersededDocumentId}</dd></div>
          </dl>
        </VersionDetails>
      </li>)}</ul>
    </section>}

    {hasPlacements && <section aria-label="Proposed placements">
      <h5>Proposed placements ({effects.placements.length})</h5>
      <ul>{effects.placements.map((placement, index) => <li key={`${placement.targetDocumentId}:${index}`}>
        <strong>{targetLabel(placement.targetDocumentId, targets)}</strong>
        <span> · {placement.beforeDocumentId ? `before ${targetLabel(placement.beforeDocumentId, targets)}` : `after ${targetLabel(placement.afterDocumentId ?? '', targets)}`}</span>
        <VersionDetails>
          <dl>
            <div><dt>Target document ID</dt><dd>{placement.targetDocumentId}</dd></div>
            {placement.beforeDocumentId && <div><dt>Before document ID</dt><dd>{placement.beforeDocumentId}</dd></div>}
            {placement.afterDocumentId && <div><dt>After document ID</dt><dd>{placement.afterDocumentId}</dd></div>}
          </dl>
        </VersionDetails>
      </li>)}</ul>
    </section>}

    {!hasAnyGroup && <details className="chat-adoption-effects-summary">
      <summary>No additional effects are recorded for this proposal</summary>
      <p className="chat-muted">The manifest contains no relationship, protected-content, impact, supersession, or placement entries beyond the document bodies shown above.</p>
    </details>}
    {hasAnyGroup && <details className="chat-adoption-effects-summary">
      <summary>Other effect categories</summary>
      <EmptyList>Empty categories are omitted from the main review to keep the proposal readable. The complete manifest remains available in the exact preview record.</EmptyList>
    </details>}
  </section>;
}
