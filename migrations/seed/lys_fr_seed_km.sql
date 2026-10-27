-- Traduction khmère (machine, km_status='draft') de la checklist lys-en-france.
-- GÉNÉRÉ depuis km.json par script : ne pas éditer à la main.
-- Rejouable : ne touche qu'aux colonnes *_km et à km_status ; jamais status, note, done, done_at, échéances.
-- Une ligne déjà relue par un humain (km_status='reviewed') n'est jamais écrasée. Chaînes en dollar-quoting $km$.
-- À jouer à la main (psql) APRÈS lys_fr_seed.sql.
BEGIN;

UPDATE lys_fr.category SET title_km = $km$ធនាគារ និងទូរស័ព្ទ$km$ WHERE slug = $km$banque-tel$km$;
UPDATE lys_fr.category SET title_km = $km$ការងារ$km$ WHERE slug = $km$emploi$km$;
UPDATE lys_fr.category SET title_km = $km$ពន្ធ$km$ WHERE slug = $km$impots$km$;
UPDATE lys_fr.category SET title_km = $km$ការកម្សាន្ត$km$ WHERE slug = $km$loisirs$km$;
UPDATE lys_fr.category SET title_km = $km$សុខភាព$km$ WHERE slug = $km$sante$km$;
UPDATE lys_fr.category SET title_km = $km$ប័ណ្ណស្នាក់នៅ (titre de séjour)$km$ WHERE slug = $km$sejour$km$;

UPDATE lys_fr.task SET title_km = $km$ការណាត់ជួបនៅធនាគារ$km$, km_status = 'draft' WHERE slug = $km$banque-rdv$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$ប័ណ្ណស្នាក់នៅច្រើនឆ្នាំ (carte pluriannuelle) « ជីវិតឯកជន និងគ្រួសារ » (vie privée et familiale)$km$, help_km = $km$VLS-TS មានសុពលភាព 1 ឆ្នាំ ហើយមិនអាចពន្យារបានទេ៖ សូមដាក់ពាក្យសុំប័ណ្ណ ចន្លោះពី 4 ខែ ដល់ 2 ខែ មុនពេល VLS-TS ផុតកំណត់។ ការប្រឡងពលរដ្ឋ (examen civique) ជាកាតព្វកិច្ច ចាប់ពីថ្ងៃ 01/01/2026 សម្រាប់ប័ណ្ណស្នាក់នៅច្រើនឆ្នាំ។ សូមបំពេញ « échéance réglementaire » (កាលកំណត់តាមច្បាប់) ដើម្បីឲ្យមានការរំលឹក។$km$, km_status = 'draft' WHERE slug = $km$carte-pluriannuelle$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$បើកសិទ្ធិធានារ៉ាប់រងសុខភាពនៅ CPAM 77 (ការិយាល័យធានារ៉ាប់រងសុខភាព) (PUMa ៖ ធានារ៉ាប់រងសុខភាពសកល) ក្នុងនាម Lys ផ្ទាល់$km$, help_km = $km$សិទ្ធិផ្ទាល់ខ្លួន (ឋានៈ « ayant droit » សម្រាប់មនុស្សពេញវ័យ ពោលគឺការមានសិទ្ធិតាមរយៈប្តីឬប្រពន្ធ លែងមានទៀតហើយ ចាប់តាំងពីមាន PUMa)។ ទម្រង់ S1106 ។ NIR (លេខសន្តិសុខសង្គម) បណ្តោះអាសន្ន រួចលេខស្ថាពរ គណនី ameli (គេហទំព័ររបស់ CPAM) និងកាត Vitale (កាតសុខភាព)។ លក្ខខណ្ឌរស់នៅជាប់លាប់ 3 ខែ អាចត្រូវលើកលែង សម្រាប់សមាជិកគ្រួសារដែលមកនៅជាមួយអ្នកមានធានារ៉ាប់រង៖ ត្រូវបញ្ជាក់ជាមួយ CPAM (à confirmer)។ ការបកប្រែដោយអ្នកបកប្រែស្បថ (traduction assermentée) នៃសំបុត្រកំណើត (acte de naissance) គឺជាជំហានដែលចំណាយពេលយូរជាងគេ។$km$, km_status = 'draft' WHERE slug = $km$cpam-puma$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$រៀបចំ CV (ប្រវត្តិរូបសង្ខេប)$km$, km_status = 'draft' WHERE slug = $km$cv$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$Disneyland, Noël Enchanté (កម្មវិធីបុណ្យណូអែល)$km$, help_km = $km$តម្លៃប្រហាក់ប្រហែល មិនទាន់បានផ្ទៀងផ្ទាត់ (non vérifié)៖ សូមពិនិត្យនៅលើគេហទំព័រ មុនពេលកក់។$km$, km_status = 'draft' WHERE slug = $km$disney-noel$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$គម្រោងទូរស័ព្ទចល័ត (forfait mobile)$km$, km_status = 'draft' WHERE slug = $km$forfait-mobile$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$France Travail (ការិយាល័យរដ្ឋសម្រាប់ស្វែងរកការងារ)$km$, help_km = $km$ត្រូវធ្វើបន្ទាប់ពីបានបញ្ជាក់សុពលភាព VLS-TS រួច។ ប័ណ្ណស្នាក់នៅ « vie privée et familiale » (ជីវិតឯកជន និងគ្រួសារ) អនុញ្ញាតឲ្យធ្វើការបាន។$km$, km_status = 'draft' WHERE slug = $km$france-travail$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$ស្វែងរកការមើលថែកុមារ (garde d'enfants) នៅ Val d'Europe$km$, km_status = 'draft' WHERE slug = $km$garde-enfants$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$ធានារ៉ាប់រងសុខភាពបន្ថែម (mutuelle) តាមក្រុមហ៊ុនរបស់ Chetana$km$, help_km = $km$បន្ថែម Lys ជាអ្នកទទួលផល (« ayant droit » តាមន័យនៃកិច្ចសន្យា)។ ពិនិត្យក្នុងឯកសារលក្ខខណ្ឌ (notice) នូវរយៈពេលកំណត់ បន្ទាប់ពីអាពាហ៍ពិពាហ៍ ឬបន្ទាប់ពីការមកដល់។$km$, km_status = 'draft' WHERE slug = $km$mutuelle$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$OFII (ការិយាល័យជាតិអន្តោប្រវេសន៍ និងសមាហរណកម្ម)៖ កិច្ចសន្យាសមាហរណកម្មសាធារណរដ្ឋ (contrat d'intégration républicaine, CIR)$km$, help_km = $km$ត្រូវធ្វើនៅពេលទទួលបានលិខិតកោះហៅ (convocation) ពី OFII ។ CIR ជាលក្ខខណ្ឌសម្រាប់ទទួលបានប័ណ្ណស្នាក់នៅច្រើនឆ្នាំ (carte pluriannuelle) នាពេលខាងមុខ។ ត្រូវបញ្ជាក់ (à confirmer)៖ លំដាប់ពិតប្រាកដនៃការកោះហៅ សម្រាប់ប្តី ឬប្រពន្ធរបស់ពលរដ្ឋបារាំង (រកមិនឃើញទំព័រផ្លូវការទេ)។$km$, km_status = 'draft' WHERE slug = $km$ofii-cir$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$SIP (ការិយាល័យពន្ធសម្រាប់បុគ្គល) នៅ Lagny-sur-Marne (3 rue Delambre, 77400)$km$, help_km = $km$ដំបូង សូមប្រកាសអាពាហ៍ពិពាហ៍តាមអ៊ីនធឺណិត (espace particulier ជាគណនីពន្ធផ្ទាល់ខ្លួន › Gérer mon prélèvement à la source › changement de situation)។ ទៅបញ្ជរ (guichet) តែនៅពេលមានការរាំងស្ទះប៉ុណ្ណោះ។ រយៈពេលកំណត់ 60 ថ្ងៃ បន្ទាប់ពីព្រឹត្តិការណ៍៖ មិនទាន់បានផ្ទៀងផ្ទាត់ (non vérifié) សម្រាប់អាពាហ៍ពិពាហ៍ដែលបានធ្វើយូរហើយ។ ត្រូវបញ្ជាក់ (à confirmer)៖ ម៉ោងបើករបស់ SIP និងថាតើ SIP នេះទទួលខុសត្រូវលើក្រុង (commune) ដែលយើងរស់នៅឬទេ (espace particulier › Contact et RDV)។$km$, km_status = 'draft' WHERE slug = $km$sip-lagny$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.task SET title_km = $km$បញ្ជាក់សុពលភាព VLS-TS (ទិដ្ឋាការស្នាក់នៅរយៈពេលវែង) នៅលើ ANEF (គេហទំព័ររដ្ឋសម្រាប់ជនបរទេស)$km$, help_km = $km$គេហទំព័រ៖ administration-etrangers-en-france.interieur.gouv.fr ។ រយៈពេលកំណត់តាមច្បាប់៖ 3 ខែ បន្ទាប់ពីថ្ងៃចូលប្រទេស (មើលត្រាចូលប្រទេស)។ បើហួសរយៈពេលនេះ មិនអាចបញ្ជាក់សុពលភាពបានទៀតទេ ហើយការស្នាក់នៅក្លាយជាមិនស្របច្បាប់៖ សូមសរសេរថ្ងៃផុតកំណត់ពិតប្រាកដ ក្នុងប្រអប់ « échéance réglementaire » (កាលកំណត់តាមច្បាប់)។ ថ្លៃពន្ធ (taxe) បង់តាមអ៊ីនធឺណិត ហើយចំនួនទឹកប្រាក់ត្រូវបានបង្ហាញដោយ ANEF (តារាងថ្លៃដែលអនុវត្តចាប់ពីថ្ងៃ 01/05/2026)។$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef$km$ AND km_status <> 'reviewed';

UPDATE lys_fr.item SET label_km = $km$ធ្វើការណាត់ជួប$km$, km_status = 'draft' WHERE slug = $km$banque-rdv.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតឆ្លងដែន (passeport) និងឯកសារបញ្ជាក់ទីលំនៅ (justificatif de domicile)$km$, km_status = 'draft' WHERE slug = $km$banque-rdv.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ដាក់ពាក្យសុំនៅលើ ANEF (4 ទៅ 2 ខែ មុនពេល VLS-TS ផុតកំណត់)$km$, km_status = 'draft' WHERE slug = $km$carte-pluriannuelle.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ប្រឡងពលរដ្ឋ (examen civique)$km$, km_status = 'draft' WHERE slug = $km$carte-pluriannuelle.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ភស្តុតាងនៃការរស់នៅជាមួយគ្នា (preuves de communauté de vie)$km$, km_status = 'draft' WHERE slug = $km$carte-pluriannuelle.3$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតបញ្ជាក់ CIR (attestation du CIR)$km$, km_status = 'draft' WHERE slug = $km$carte-pluriannuelle.4$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$បំពេញទម្រង់ S1106$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ទទួល NIR (លេខសន្តិសុខសង្គម) បណ្តោះអាសន្ន រួចលេខស្ថាពរ$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$បង្កើតគណនី ameli$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.3$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ទទួលកាត Vitale$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.4$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតឆ្លងដែន (passeport) និងលិខិតបញ្ជាក់សុពលភាព VLS-TS (attestation de validation du VLS-TS)$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.5$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$សំបុត្រកំណើត (acte de naissance) ច្បាប់ចម្លងពេញលេញ មានឈ្មោះឪពុកម្តាយ (copie intégrale avec filiation) និងការបកប្រែដោយអ្នកបកប្រែស្បថ (traducteur assermenté)$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.6$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$សំបុត្រអាពាហ៍ពិពាហ៍ (acte de mariage)$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.7$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ឯកសារបញ្ជាក់ទីលំនៅ (justificatif de domicile) និង RIB (ឯកសារលេខគណនីធនាគារ)$km$, km_status = 'draft' WHERE slug = $km$cpam-puma.8$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$សរសេរ CV$km$, km_status = 'draft' WHERE slug = $km$cv.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$កក់សំបុត្រដែលមានកាលបរិច្ឆេទ$km$, km_status = 'draft' WHERE slug = $km$disney-noel.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ជ្រើសរើស និងចុះឈ្មោះប្រើគម្រោងទូរស័ព្ទ (forfait)$km$, km_status = 'draft' WHERE slug = $km$forfait-mobile.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតឆ្លងដែន (passeport) និងឯកសារបញ្ជាក់ទីលំនៅ (justificatif de domicile)$km$, km_status = 'draft' WHERE slug = $km$forfait-mobile.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ចុះឈ្មោះនៅ France Travail$km$, km_status = 'draft' WHERE slug = $km$france-travail.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតបញ្ជាក់សុពលភាព VLS-TS (attestation de validation du VLS-TS)$km$, km_status = 'draft' WHERE slug = $km$france-travail.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ស្វែងរកការមើលថែកុមារ (garde d'enfants)$km$, km_status = 'draft' WHERE slug = $km$garde-enfants.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$បន្ថែម Lys ជាអ្នកទទួលផល$km$, km_status = 'draft' WHERE slug = $km$mutuelle.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ពិនិត្យរយៈពេលកំណត់ដែលមានចែងក្នុងឯកសារលក្ខខណ្ឌ (notice)$km$, km_status = 'draft' WHERE slug = $km$mutuelle.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$សំបុត្រអាពាហ៍ពិពាហ៍ (acte de mariage) និង NIR របស់ Lys (នៅពេលមាន)$km$, km_status = 'draft' WHERE slug = $km$mutuelle.3$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ឆ្លើយតបលិខិតកោះហៅ (convocation) របស់ OFII$km$, km_status = 'draft' WHERE slug = $km$ofii-cir.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ការសម្ភាសន៍ និងការធ្វើតេស្តភាសាបារាំង$km$, km_status = 'draft' WHERE slug = $km$ofii-cir.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ចុះហត្ថលេខាលើ CIR$km$, km_status = 'draft' WHERE slug = $km$ofii-cir.3$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ចូលរៀនវគ្គបណ្តុះបណ្តាលពលរដ្ឋ (formation civique) (4 ថ្ងៃ)$km$, km_status = 'draft' WHERE slug = $km$ofii-cir.4$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតឆ្លងដែន (passeport) និងលិខិតបញ្ជាក់សុពលភាព VLS-TS (attestation de validation du VLS-TS)$km$, km_status = 'draft' WHERE slug = $km$ofii-cir.5$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ប្រកាសអាពាហ៍ពិពាហ៍តាមអ៊ីនធឺណិត (changement de situation ៖ ការផ្លាស់ប្តូរស្ថានភាព)$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ទទួលលេខពន្ធ (numéro fiscal) របស់ Lys (នៅ SIP បើនាងមិនទាន់មានលេខនេះ)$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ពិនិត្យអត្រាថ្មីនៃការកាត់ពន្ធពីប្រាក់ចំណូលដោយផ្ទាល់ (prélèvement à la source) (គ្រួសារ (foyer), 2 ចំណែក (parts))$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.3$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតឆ្លងដែនរបស់ប្តីប្រពន្ធ (passeports)$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.4$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$VLS-TS ដែលបានបញ្ជាក់សុពលភាពរួច (VLS-TS validé)$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.5$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$សំបុត្រអាពាហ៍ពិពាហ៍ដែលបានចុះបញ្ជីនៅប្រទេសបារាំង (acte de mariage transcrit)$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.6$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ឯកសារបញ្ជាក់ទីលំនៅរួមគ្នា (justificatif de domicile commun)$km$, km_status = 'draft' WHERE slug = $km$sip-lagny.7$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$បង្កើតគណនី ANEF$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.1$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$បញ្ចូលទិដ្ឋាការ ថ្ងៃចូលប្រទេស និងអាសយដ្ឋាននៅប្រទេសបារាំង$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.2$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$បង់ថ្លៃពន្ធ (taxe) តាមអ៊ីនធឺណិត (ចំនួនទឹកប្រាក់ត្រូវបានបង្ហាញដោយ ANEF)$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.3$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ទាញយកលិខិតបញ្ជាក់សុពលភាព (attestation de validation)$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.4$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$លិខិតឆ្លងដែនដែលមានទិដ្ឋាការ (passeport avec le visa)$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.5$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ត្រាចូលប្រទេស (tampon d'entrée) (ថតរូប)$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.6$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$ឯកសារបញ្ជាក់ទីលំនៅ (justificatif de domicile)$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.7$km$ AND km_status <> 'reviewed';
UPDATE lys_fr.item SET label_km = $km$កាតធនាគារ និងអាសយដ្ឋានអ៊ីមែល$km$, km_status = 'draft' WHERE slug = $km$vls-ts-anef.8$km$ AND km_status <> 'reviewed';

COMMIT;
