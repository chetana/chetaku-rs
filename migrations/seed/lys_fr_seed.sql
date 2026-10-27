-- Contenu FR de la checklist lys-en-france. Rejouable sans jamais écraser l'état :
-- UPSERT sur slug, seules les colonnes de contenu sont mises à jour ; status, note, done, done_at,
-- due_override, legal_deadline et les colonnes *_km ne sont jamais touchés.
-- Montants de taxe volontairement absents (barème en vigueur depuis le 01/05/2026, à lire sur l'ANEF).
-- Joué à la main (psql), hors du dossier lu par sqlx.
BEGIN;

INSERT INTO lys_fr.category (slug, position, title_fr) VALUES
  ('sejour',     1, 'Titre de séjour'),
  ('impots',     2, 'Impôts'),
  ('sante',      3, 'Santé'),
  ('banque-tel', 4, 'Banque et téléphone'),
  ('emploi',     5, 'Emploi'),
  ('loisirs',    6, 'Loisirs')
ON CONFLICT (slug) DO UPDATE SET position = EXCLUDED.position, title_fr = EXCLUDED.title_fr;

INSERT INTO lys_fr.task (slug, category_slug, position, title_fr, help_fr, due_date) VALUES
  ('vls-ts-anef', 'sejour', 1,
   'Valider le VLS-TS sur l''ANEF',
   'Site : administration-etrangers-en-france.interieur.gouv.fr. Délai réglementaire : 3 mois après la date d''entrée (tampon). Passé ce délai, la validation n''est plus possible et le séjour devient irrégulier : saisir la date limite exacte dans « échéance réglementaire ». La taxe est payée en ligne ; le montant est affiché par l''ANEF (barème en vigueur depuis le 01/05/2026).',
   DATE '2026-11-09'),
  ('ofii-cir', 'sejour', 2,
   'OFII : contrat d''intégration républicaine (CIR)',
   'À faire à réception de la convocation de l''OFII. Le CIR conditionne la future carte pluriannuelle. À confirmer : ordre exact des convocations pour le conjoint d''un Français (pas de page officielle trouvée).',
   NULL),
  ('carte-pluriannuelle', 'sejour', 3,
   'Carte pluriannuelle « vie privée et familiale »',
   'Le VLS-TS dure 1 an et n''est pas prolongeable : demander la carte entre 4 et 2 mois avant sa fin. Examen civique obligatoire depuis le 01/01/2026 pour une carte pluriannuelle. Renseigner l''échéance réglementaire pour déclencher un rappel.',
   NULL),
  ('sip-lagny', 'impots', 1,
   'SIP de Lagny-sur-Marne (3 rue Delambre, 77400)',
   'Déclarer d''abord le mariage en ligne (espace particulier, Gérer mon prélèvement à la source, changement de situation) ; le guichet seulement si c''est bloqué. Délai de 60 jours après l''événement : non vérifié pour un mariage déjà ancien. À confirmer : horaires du SIP, et sa compétence pour la commune de résidence (espace particulier, Contact et RDV).',
   DATE '2026-11-06'),
  ('cpam-puma', 'sante', 1,
   'Ouverture des droits CPAM 77 (PUMa) au nom de Lys',
   'Droits en propre (la qualité d''ayant droit majeur n''existe plus depuis la PUMa). Formulaire S1106. NIR provisoire puis définitif, compte ameli, carte Vitale. La condition de 3 mois de résidence stable peut être levée pour un membre de famille qui rejoint un assuré : à confirmer avec la CPAM. La traduction assermentée de l''acte de naissance est le point le plus long.',
   DATE '2026-11-30'),
  ('mutuelle', 'sante', 2,
   'Mutuelle d''entreprise de Chetana',
   'Ajouter Lys comme bénéficiaire (ayant droit au sens du contrat). Vérifier dans la notice le délai suivant le mariage ou l''arrivée.',
   DATE '2026-11-30'),
  ('banque-rdv', 'banque-tel', 1, 'Rendez-vous à la banque', NULL, NULL),
  ('forfait-mobile', 'banque-tel', 2, 'Forfait mobile', NULL, NULL),
  ('france-travail', 'emploi', 1,
   'France Travail',
   'À faire après la validation du VLS-TS. Le titre « vie privée et familiale » autorise à travailler.',
   NULL),
  ('garde-enfants', 'emploi', 2, 'Recherche de garde d''enfants à Val d''Europe', NULL, NULL),
  ('cv', 'emploi', 3, 'Préparer le CV', NULL, NULL),
  ('disney-noel', 'loisirs', 1,
   'Disneyland, Noël Enchanté',
   'Prix indicatif non vérifié : à vérifier sur le site avant de réserver.',
   DATE '2026-11-30')
ON CONFLICT (slug) DO UPDATE SET
  category_slug = EXCLUDED.category_slug, position = EXCLUDED.position,
  title_fr = EXCLUDED.title_fr, help_fr = EXCLUDED.help_fr, due_date = EXCLUDED.due_date;

INSERT INTO lys_fr.item (slug, task_slug, kind, position, label_fr) VALUES
  ('vls-ts-anef.1', 'vls-ts-anef', 'step',     1, 'Créer le compte ANEF'),
  ('vls-ts-anef.2', 'vls-ts-anef', 'step',     2, 'Saisir le visa, la date d''entrée et l''adresse en France'),
  ('vls-ts-anef.3', 'vls-ts-anef', 'step',     3, 'Payer la taxe en ligne (montant affiché par l''ANEF)'),
  ('vls-ts-anef.4', 'vls-ts-anef', 'step',     4, 'Télécharger l''attestation de validation'),
  ('vls-ts-anef.5', 'vls-ts-anef', 'document', 5, 'Passeport avec le visa'),
  ('vls-ts-anef.6', 'vls-ts-anef', 'document', 6, 'Tampon d''entrée (photo)'),
  ('vls-ts-anef.7', 'vls-ts-anef', 'document', 7, 'Justificatif de domicile'),
  ('vls-ts-anef.8', 'vls-ts-anef', 'document', 8, 'Carte bancaire et adresse e-mail'),

  ('ofii-cir.1', 'ofii-cir', 'step',     1, 'Répondre à la convocation de l''OFII'),
  ('ofii-cir.2', 'ofii-cir', 'step',     2, 'Entretien et test de français'),
  ('ofii-cir.3', 'ofii-cir', 'step',     3, 'Signer le CIR'),
  ('ofii-cir.4', 'ofii-cir', 'step',     4, 'Suivre la formation civique (4 jours)'),
  ('ofii-cir.5', 'ofii-cir', 'document', 5, 'Passeport et attestation de validation du VLS-TS'),

  ('carte-pluriannuelle.1', 'carte-pluriannuelle', 'step',     1, 'Faire la demande sur l''ANEF (4 à 2 mois avant la fin du VLS-TS)'),
  ('carte-pluriannuelle.2', 'carte-pluriannuelle', 'step',     2, 'Passer l''examen civique'),
  ('carte-pluriannuelle.3', 'carte-pluriannuelle', 'document', 3, 'Preuves de communauté de vie'),
  ('carte-pluriannuelle.4', 'carte-pluriannuelle', 'document', 4, 'Attestation du CIR'),

  ('sip-lagny.1', 'sip-lagny', 'step',     1, 'Déclarer le mariage en ligne (changement de situation)'),
  ('sip-lagny.2', 'sip-lagny', 'step',     2, 'Obtenir le numéro fiscal de Lys (au SIP si elle n''en a pas)'),
  ('sip-lagny.3', 'sip-lagny', 'step',     3, 'Vérifier le nouveau taux de prélèvement à la source (foyer, 2 parts)'),
  ('sip-lagny.4', 'sip-lagny', 'document', 4, 'Passeports'),
  ('sip-lagny.5', 'sip-lagny', 'document', 5, 'VLS-TS validé'),
  ('sip-lagny.6', 'sip-lagny', 'document', 6, 'Acte de mariage transcrit'),
  ('sip-lagny.7', 'sip-lagny', 'document', 7, 'Justificatif de domicile commun'),

  ('cpam-puma.1', 'cpam-puma', 'step',     1, 'Remplir le formulaire S1106'),
  ('cpam-puma.2', 'cpam-puma', 'step',     2, 'Obtenir le NIR provisoire puis définitif'),
  ('cpam-puma.3', 'cpam-puma', 'step',     3, 'Créer le compte ameli'),
  ('cpam-puma.4', 'cpam-puma', 'step',     4, 'Recevoir la carte Vitale'),
  ('cpam-puma.5', 'cpam-puma', 'document', 5, 'Passeport et attestation de validation du VLS-TS'),
  ('cpam-puma.6', 'cpam-puma', 'document', 6, 'Acte de naissance (copie intégrale avec filiation) et traduction par un traducteur assermenté'),
  ('cpam-puma.7', 'cpam-puma', 'document', 7, 'Acte de mariage'),
  ('cpam-puma.8', 'cpam-puma', 'document', 8, 'Justificatif de domicile et RIB'),

  ('mutuelle.1', 'mutuelle', 'step',     1, 'Ajouter Lys comme bénéficiaire'),
  ('mutuelle.2', 'mutuelle', 'step',     2, 'Vérifier le délai prévu par la notice'),
  ('mutuelle.3', 'mutuelle', 'document', 3, 'Acte de mariage et NIR de Lys (quand il existe)'),

  ('banque-rdv.1', 'banque-rdv', 'step',     1, 'Prendre rendez-vous'),
  ('banque-rdv.2', 'banque-rdv', 'document', 2, 'Passeport et justificatif de domicile'),
  ('forfait-mobile.1', 'forfait-mobile', 'step',     1, 'Choisir et souscrire un forfait'),
  ('forfait-mobile.2', 'forfait-mobile', 'document', 2, 'Passeport et justificatif de domicile'),

  ('france-travail.1', 'france-travail', 'step',     1, 'S''inscrire à France Travail'),
  ('france-travail.2', 'france-travail', 'document', 2, 'Attestation de validation du VLS-TS'),
  ('garde-enfants.1',  'garde-enfants',  'step',     1, 'Chercher une garde d''enfants'),
  ('cv.1',             'cv',             'step',     1, 'Écrire le CV'),
  ('disney-noel.1',    'disney-noel',    'step',     1, 'Réserver un billet daté')
ON CONFLICT (slug) DO UPDATE SET
  task_slug = EXCLUDED.task_slug, kind = EXCLUDED.kind, position = EXCLUDED.position, label_fr = EXCLUDED.label_fr;

COMMIT;
