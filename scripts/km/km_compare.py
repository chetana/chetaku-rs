"""Compare FR source / KM / rétro-traduction : nombres, noms propres, négations, marqueurs."""
import json, re, sys
from collections import Counter
from km_tools import pairs, SEED

NAMES = ["ANEF", "OFII", "CIR", "VLS-TS", "SIP", "CPAM", "PUMa", "NIR", "Vitale", "ameli", "S1106",
         "France Travail", "Lagny-sur-Marne", "Delambre", "Val d'Europe", "Disneyland", "Noël Enchanté",
         "Lys", "Chetana", "RIB", "CV", "administration-etrangers-en-france.interieur.gouv.fr",
         "Gérer mon prélèvement à la source", "Contact et RDV", "impots.gouv.fr", "Kinougarde", "Educazen"]
MARKERS = ["à confirmer", "non vérifié"]
NUM = re.compile(r"(?<![A-Za-z\d])\d+(?:/\d+)*")
NEG_NE = re.compile(r"(?i)(?<![\w])(n'|ne(?![\w]))")
NEG_OTHER = re.compile(r"(?i)(?<![\w])(non|aucune?|sans|jamais)(?![\w])")
PAS = re.compile(r"(?i)(?<![\w])pas(?![\w])")

def neg_units(s):
    ne = len(NEG_NE.findall(s))
    return ne + len(NEG_OTHER.findall(s)) + max(0, len(PAS.findall(s)) - ne)

def names(s):
    return Counter(n for n in NAMES if re.search(r"(?<![\w-])" + re.escape(n) + r"(?![\w-])", s))

def main():
    retro = json.loads((SEED / "km-retro.json").read_text())
    ecarts = []
    for table, slug, f, k, fr, km in pairs():
        rt = retro.get(table, {}).get(slug, {}).get(k)
        key = f"{table}/{slug}/{f}"
        if not rt:
            ecarts.append((key, "rétro-traduction absente", "", "")); continue
        for label, a, b in (("nombres FR≠KM", NUM.findall(fr), NUM.findall(km)),
                            ("nombres FR≠rétro", NUM.findall(fr), NUM.findall(rt))):
            if Counter(a) != Counter(b):
                ecarts.append((key, label, " ".join(a), " ".join(b)))
        nf = set(names(fr))
        for label, other in (("noms FR≠KM", km), ("noms FR≠rétro", rt)):
            no = set(names(other))
            if nf != no:
                ecarts.append((key, label, f"manque: {sorted(nf-no)}", f"ajout: {sorted(no-nf)}"))
        nneg_fr, nneg_rt = neg_units(fr), neg_units(rt.replace('(non vérifié)', ''))  # le marqueur FR recopié n'est pas une 2e négation
        if nneg_fr != nneg_rt:
            ecarts.append((key, "négations", f"FR={nneg_fr}", f"rétro={nneg_rt}"))
        for m in MARKERS:
            if m in fr.lower() and (m not in km.lower() or m not in rt.lower()):
                ecarts.append((key, f"marqueur « {m} » perdu", "", ""))
    out = ["# Écarts FR source / rétro-traduction du khmer", "",
           "Généré par `km_compare.py` (contrôles mécaniques : nombres/dates/montants, noms propres et organismes, "
           "nombre de négations, marqueurs « à confirmer » / « non vérifié »).", "",
           f"**{len(ecarts)} écart(s) mécanique(s).**", ""]
    if ecarts:
        out += ["| Texte | Contrôle | Source | Rétro/KM |", "|---|---|---|---|"]
        out += [f"| `{a}` | {b} | {c} | {d} |" for a, b, c, d in ecarts]
    out += ["", "## Relecture de sens (manuelle, rétro vs FR)", "",
        "Itérations : 1er passage = 5 écarts mécaniques (dont 3 faux positifs du contrôle des négations « n' »), "
        "corrigés : doublon du nombre « 2 » dans `sip-lagny.3`, reformulation négative du guichet dans `sip-lagny` "
        "(« si on n'y arrive pas en ligne » → « en cas de blocage »), libellé redondant « attestation de la confirmation "
        "de validité » (×4) simplifié. 2e et 3e passages : 0 écart.", "",
        "Points douteux restants (sens à faire relire par Lys) :",
        "- `garde-enfants` / `garde-enfants.1` : le FR est ambigu (chercher un emploi de garde d'enfants, rubrique Emploi, "
        "ou chercher un mode de garde ?). Le khmer garde la même ambiguïté (« chercher la garde d'enfants »).",
        "- `sip-lagny.4` : « Passeports » traduit par « passeports du couple » (interprétation du pluriel).",
        "- « valider le VLS-TS » rendu par បញ្ជាក់សុពលភាព (confirmer la validité) : terme le plus courant, pas de terme officiel khmer.",
        "- Explications ajoutées entre parenthèses (ANEF, OFII, SIP, CPAM, PUMa, NIR, ameli, Vitale, France Travail, RIB, CV) : "
        "ce sont des gloses, pas des traductions officielles.",
        "- Registre : textes à la 2e personne polie (សូម) ; le « nous » de `sip-lagny` (commune où nous habitons) est conservé."]
    (SEED / "km-ecarts.md").write_text("\n".join(out) + "\n")
    for e in ecarts: print(e)
    print(f"{len(ecarts)} écart(s)")

if __name__ == "__main__":
    main()
