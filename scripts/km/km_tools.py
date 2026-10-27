"""Outils khmer lys-en-france : parse du seed FR, couverture, génération SQL."""
import json, re, sys
from pathlib import Path

SEED = Path.home() / "Chetana/chetaku-rs/migrations/seed"

def sql_tuples(block):
    """Découpe 'VALUES (...),(...)' en listes de valeurs (str|None|int|DATE)."""
    out, i, n = [], 0, len(block)
    while i < n:
        if block[i] == '(':
            vals, cur, i = [], None, i + 1
            while block[i] != ')':
                c = block[i]
                if c == "'":
                    j, s = i + 1, []
                    while True:
                        if block[j] == "'" and j + 1 < n and block[j+1] == "'":
                            s.append("'"); j += 2
                        elif block[j] == "'":
                            break
                        else:
                            s.append(block[j]); j += 1
                    cur = "".join(s); i = j + 1
                elif c == ',':
                    vals.append(cur); cur = None; i += 1
                elif c.isspace():
                    i += 1
                else:
                    m = re.match(r"(NULL|DATE|\d+)", block[i:])
                    tok = m.group(1); i += len(tok)
                    if tok == "NULL": cur = None
                    elif tok.isdigit(): cur = int(tok)
            vals.append(cur); out.append(vals); i += 1
        else:
            i += 1
    return out

def load_fr():
    sql = (SEED / "lys_fr_seed.sql").read_text()
    parts = re.findall(r"INSERT INTO lys_fr\.(\w+) \(([^)]*)\) VALUES(.*?)ON CONFLICT", sql, re.S)
    fr = {}
    for table, cols, body in parts:
        cols = [c.strip() for c in cols.split(",")]
        fr[table] = {}
        for t in sql_tuples(body):
            row = dict(zip(cols, t))
            fr[table][row["slug"]] = row
    return fr

# champ FR -> champ KM du JSON
FIELDS = {"category": [("title_fr", "title_km")],
          "task": [("title_fr", "title_km"), ("help_fr", "help_km")],
          "item": [("label_fr", "text_km")]}

def pairs():
    fr, km = load_fr(), json.loads((SEED / "km.json").read_text())
    for table, fl in FIELDS.items():
        for slug, row in fr[table].items():
            for f, k in fl:
                if row.get(f) is not None:
                    yield table, slug, f, k, row[f], km.get(table, {}).get(slug, {}).get(k)

def coverage():
    fr, km = load_fr(), json.loads((SEED / "km.json").read_text())
    exp = list(pairs()); missing = [p[:4] for p in exp if not p[5] or not p[5].strip()]
    extra = [(t, s) for t in km for s in km[t] if s not in fr.get(t, {})]
    extra_f = [(t, s, k) for t in km for s, d in km[t].items() for k in d
               if s in fr.get(t, {}) and k not in [kk for f, kk in FIELDS[t] if fr[t][s].get(f) is not None]]
    no_khmer = [p[:4] for p in exp if p[5] and not re.search(r"[ក-៿]", p[5])]
    print(f"attendus={len(exp)} traduits={len(exp)-len(missing)} manquants={missing} slugs_en_trop={extra} champs_en_trop={extra_f} sans_khmer={no_khmer}")
    return not (missing or extra or extra_f or no_khmer)

def gen_sql():
    tag = "km"
    def q(s):
        assert f"${tag}$" not in s
        return f"${tag}${s}${tag}$"
    col = {"category": {"title_km": "title_km"},
           "task": {"title_km": "title_km", "help_km": "help_km"},
           "item": {"text_km": "label_km"}}
    km = json.loads((SEED / "km.json").read_text())
    L = ["-- Traduction khmère (machine, km_status='draft') de la checklist lys-en-france.",
         "-- GÉNÉRÉ depuis km.json par script : ne pas éditer à la main.",
         "-- Rejouable : ne touche qu'aux colonnes *_km et à km_status ; jamais status, note, done, done_at, échéances.",
         "-- Une ligne déjà relue par un humain (km_status='reviewed') n'est jamais écrasée. Chaînes en dollar-quoting $km$.",
         "-- À jouer à la main (psql) APRÈS lys_fr_seed.sql.", "BEGIN;", ""]
    for table in ("category", "task", "item"):
        for slug in sorted(km[table]):
            d = km[table][slug]
            sets = [f"{col[table][k]} = {q(v)}" for k, v in d.items()]
            if table != "category":
                sets.append("km_status = 'draft'")
            guard = " AND km_status <> 'reviewed'" if table != "category" else ""
            L.append(f"UPDATE lys_fr.{table} SET {', '.join(sets)} WHERE slug = {q(slug)}{guard};")
        L.append("")
    L.append("COMMIT;")
    (SEED / "lys_fr_seed_km.sql").write_text("\n".join(L) + "\n")
    print("SQL écrit")

if __name__ == "__main__":
    {"coverage": lambda: sys.exit(0 if coverage() else 1), "sql": gen_sql}[sys.argv[1]]()
