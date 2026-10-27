# Effects (`effect.scs`)

What the shader archive contains. It is read through `crates/scs-archive`; no dedicated parser exists yet. Material files name an effect (see [models.md](models.md#mat-materials)), and this archive maps each effect to shaders.

**Indexing evidence:** the `survey` of `effect.scs` reaches 24,481 entries with 0 skipped. This needs metadata record `0x07`, see [hashfs.md](hashfs.md#metadata-records).

| Type | Count | Contents |
|---|---|---|
| `.rfx` | 8,970 | Text effect descriptions, e.g. `/effect/eut2/truckpaint/eut2.truckpaint.airbrush.altuv.asafew.a.rfx` |
| `.vso` | 4,854 | Vertex shaders (per API) |
| `.fso` | 10,433 | Fragment shaders (per API) |
| `.rs` | 111 | Render-state files |

Each `.rfx` file holds:

```text
effect "<name>" { render { flags: N  technique_count: N
  technique { predicate { path: glsl }  pass_count: N
    pass { semantics: "defattr" | "fwdopaque" | ...  profile: "material"
           state_setup: "/effect/deferred/rs/*.rs"
           vertex_setup: "/effect/eut2/_shd/<hash>.glsl.vso"
           fragment_setup: "/effect/eut2/_shd/<hash>.glsl.fso" } } } }
```

So the effect name in a `.mat` maps to concrete shaders per API (`predicate.path`).

**Hypothesis:** the `.glsl` variants are readable GLSL 330. This comes from peer research in `research/evidence/effect-shaders-survey.txt` and stays a hypothesis until our parser reads them.
