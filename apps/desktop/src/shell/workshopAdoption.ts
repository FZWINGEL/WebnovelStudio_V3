/** Workshop adoption flow: form state, preview preparation, and the committed adopt call. */
import { useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import { readDocument, type DocumentRecord, type ProjectAccess } from '../ipc/projects';
import { adoptWorkshop, previewWorkshopAdoption, type WorkshopAdoptionPreview, type WorkshopAdoptionTarget, type WorkshopImpactDraft, type WorkshopSession, type WorkshopState } from '../ipc/workshop';
import { adoptionParticipants, validAdoptionLinks, type AdoptionMaterialDraft, type AdoptionLinkDraft } from '../workshop';
import { appendText, textDocument } from '../workshop';
import { selectedBranchCandidates } from '../workshop';
import type { WorkshopStore } from '../workshop';
export interface WorkshopAdoptionContext {
  access: ProjectAccess;
  session: WorkshopSession | undefined;
  state: WorkshopState;
  store: WorkshopStore;
  material: DocumentRecord[];
  composing: MutableRefObject<boolean>;
  setNotice: Dispatch<SetStateAction<string>>;
  report(reason: unknown): void;
  onDocumentsChanged(documents: DocumentRecord[]): void;
}

export function useWorkshopAdoption(context: WorkshopAdoptionContext) {
  const { access, session, state, store, material, composing, setNotice, report, onDocumentsChanged } = context;
  const [adopting, setAdopting] = useState(false);
  const [preview, setPreview] = useState<WorkshopAdoptionPreview | null>(null);
  const [adoptionForm, setAdoptionForm] = useState(false);
  const [targets, setTargets] = useState<AdoptionMaterialDraft[]>([]);
  const [adoptionLinks, setAdoptionLinks] = useState<AdoptionLinkDraft[]>([]);
  const [impactDrafts, setImpactDrafts] = useState<WorkshopImpactDraft[]>([]);
  const [rationale, setRationale] = useState('');
  const adoptionOperation = useRef<string | null>(null);
  const previewGeneration = useRef(0);
  const participants = adoptionParticipants(material, targets);

  function beginAdoption() {
    if (!session || !session.workingText.trim()) return;
    const kind = session.lens === 'world' ? 'world' : session.lens === 'people' ? 'character' : session.lens === 'themes' ? 'theme' : session.lens === 'possibilities' ? 'hook' : 'note';
    setTargets(session.relationshipId ? [] : [{ id: crypto.randomUUID(), documentId: session.focusDocumentId ?? '', title: session.workingTitle || session.title, kind, mode: 'add', text: session.workingText }]);
    setAdoptionLinks([]);
    const affected = new Map<string, WorkshopImpactDraft>();
    for (const { candidate } of selectedBranchCandidates(state, session, store.results)) {
      for (const target of candidate.affectedTargets) {
        // The blank request anchor is internal bookkeeping, not story material.
        if (target.documentId.startsWith('workshop-')) continue;
        const prior = affected.get(target.documentId);
        affected.set(target.documentId, { documentId: target.documentId, kind: 'possibleTension', reason: prior ? `${prior.reason}\n${target.reason}` : target.reason });
      }
    }
    setImpactDrafts([...affected.values()]);
    setRationale(''); setPreview(null); setAdoptionForm(true);
  }

  async function prepareAdoption() {
    if (!session || composing.current) return;
    if (!targets.length || targets.some(target => !target.text.trim() || !target.documentId && !target.title.trim())) { setNotice('Choose where this version belongs and review its content before previewing.'); return; }
    if (!validAdoptionLinks(adoptionLinks, participants)) { setNotice('Review the relationship participants and descriptions before previewing.'); return; }
    setAdopting(true);
    try {
      await store.flush();
      const proposalTargets: WorkshopAdoptionTarget[] = await Promise.all(targets.map(async target => {
        const source = target.documentId ? await readDocument(access, target.documentId) : null;
        return { documentId: source?.head.documentId ?? target.id, expected: source?.head ?? null, title: source?.title ?? target.title, kind: source?.kind ?? target.kind, mode: target.mode, body: source && target.mode === 'add' ? appendText(source.body, target.text) : textDocument(target.text) };
      }));
      const current = store.state.sessions.find(item => item.id === session.id)!;
      const relationships = adoptionLinks.map(link => ({ ...link,
        fromExpected: material.find(document => document.head.documentId === link.fromDocumentId)?.head ?? null,
        toExpected: material.find(document => document.head.documentId === link.toDocumentId)?.head ?? null,
      }));
      const value = await previewWorkshopAdoption({ access, sessionId: current.id, expectedVersion: store.version, candidateIds: [...new Set(current.selectedDetails.flatMap(detail => detail.candidateId ? [detail.candidateId] : []))], targets: proposalTargets, rationale, protectedText: current.selectedDetails.filter(detail => detail.fixed).map(detail => detail.text), relationships, impactDrafts });
      previewGeneration.current = store.generation; setPreview(value); setAdoptionForm(false);
    } catch (reason) { report(reason); }
    finally { setAdopting(false); }
  }

  async function commitAdoption() {
    if (!preview || composing.current) return;
    setAdopting(true);
    try {
      if (!adoptionOperation.current) {
        await store.flush();
        if (store.version !== preview.expectedVersion) throw new Error('Your exploration changed after preview. Prepare the adoption again.');
        adoptionOperation.current = crypto.randomUUID();
      }
      const ack = await adoptWorkshop(access, adoptionOperation.current, preview.id);
      store.acceptAdoption(ack.snapshot, previewGeneration.current); adoptionOperation.current = null; store.setInteractionLocked(false);
      onDocumentsChanged(ack.documents); setPreview(null); setNotice('Version chosen. Its source and rationale are saved; writing access remains author only.');
    } catch (reason) {
      const code = reason && typeof reason === 'object' && 'code' in reason ? String(reason.code) : '';
      if (code && !['UncertainOutcome', 'PersistenceUnavailable', 'ActorUnavailable'].includes(code)) { adoptionOperation.current = null; store.setInteractionLocked(false); }
      report(reason);
    }
    finally { setAdopting(false); }
  }

  return {
    adopting, preview, adoptionForm, targets, adoptionLinks, impactDrafts, rationale,
    adoptionOperation, previewGeneration, participants,
    setAdoptionForm, setPreview, setTargets, setAdoptionLinks, setImpactDrafts, setRationale,
    beginAdoption, prepareAdoption, commitAdoption,
  };
}
