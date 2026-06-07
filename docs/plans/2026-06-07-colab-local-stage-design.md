# Colab local-stage dataset before training — design

Date: 2026-06-07

## Goal

Eliminate all Google Drive / rclone reads **during the train phase**. Stage the
entire dataset to Colab local disk (`/content/dataset`) once, upfront; training
reads only local files. Both Colab notebooks converge on a single local training
path with two interchangeable staging front-ends.

### Motivation

The ablation notebook (`colab/ba283_ab_colab.ipynb`) currently mounts the Drive
folder via an `rclone mount` FUSE layer with VFS full-cache, warms it with
`find | xargs cat`, then trains **reading through the mount**. Risks: per-file
FUSE latency, reads racing with VFS eviction / network on long (200-epoch) runs,
and a silent dependency on the mount staying healthy for the whole run. Copying
the dataset down once, then reading plain local files, removes that class of
failure entirely.

## Architecture

New tested module **`train/stage.py`**. Pure-python, no Colab/torch deps, so it
unit-tests locally. Ships in the repo clone (used by `ba283_ab_colab`) and in the
`make_bundle.sh` zip (used by `ba283_train_colab`), since the bundle already zips
the `train/` package.

```python
stage_zip(zip_path, dest="/content/dataset")   -> dataset_dir   # unzip front-end
stage_rclone(remote, dest="/content/dataset")  -> dataset_dir   # rclone copy front-end
remap_manifests(dataset_dir,
                names=("train", "val", "test"),
                out_dir="/content")            -> dict[str, str] # name -> manifest path
```

- **`stage_rclone`** shells `rclone copy <remote> <dest> -P` — **copy, not mount.**
  No VFS, no FUSE, no `find | cat` warm step. Asserts `manifest.csv` is present in
  `dest` after the copy.
- **`stage_zip`** extracts the uploaded bundle zip to `dest` and returns the data
  root (handles the bundle's `data/` prefix so the returned dir contains
  `train.csv`, `dry/`, `wet/`).
- **`remap_manifests`** reads each `<dataset_dir>/<name>.csv`, rewrites the
  `dry_path` / `wet_path` columns to `<dataset_dir>/<dry|wet-subdir>/<basename>`
  (absolute local paths), writes the rewritten CSVs to `out_dir`, and asserts the
  row-0 `wet_path` exists before returning. Returns the new manifest paths to feed
  `--train-manifest` / `--val-manifest`.

Rationale for explicit remap: the box writes absolute paths into the manifests,
and the loader's `_resolve_path` anchors only relative paths to the manifest
parent / repo root. Rewriting to absolute local paths is the robust move and
matches the logic already inline in the current ab_colab remap cell.

## Data flow

```
Drive remote ──rclone copy──┐
                            ├─► /content/dataset ──remap──► /content/{train,val,test}.csv ──► train.train (local reads only)
zip bundle ───unzip─────────┘
```

Both front-ends produce the identical `/content/dataset` layout plus remapped
manifests in `/content`, after which every downstream cell (train, eval, export)
is staging-agnostic.

## Notebook changes

### `colab/ba283_ab_colab.ipynb`

- Keep the rclone install + `rclone.conf` upload (still needed: `rclone` is used to
  copy the dataset **down** and to copy results **up**).
- Replace the `rclone mount` + VFS + `find|cat` warm cell with a `stage_rclone(...)`
  call that copies the dataset to `/content/dataset`.
- Replace the inline manifest-remap cell with `remap_manifests(...)`.
- Result-upload cells keep `rclone copy` to Drive. Cell 14 currently writes exports
  *to the mount* (`DATA + '/ba283_*.json'`); with no mount, it stages exports to a
  local dir and `rclone copy`s them up — same pattern as cell 10.

### `colab/ba283_train_colab.ipynb`

- Keep `files.upload()` of `ba283_train_bundle.zip`.
- Route through `stage_zip(...)` + `remap_manifests(...)` instead of raw `unzip` +
  reliance on relative-path resolution, so it trains against the same `/content`
  local manifests as the ablation notebook.

## Error handling

- Missing `manifest.csv` after copy/unzip, or missing `rclone.conf`, → assert with
  a clear message (as today).
- `rclone copy` non-zero exit → raise, surfacing the tail of stderr.
- `remap_manifests` row-0 existence assert catches a bad/partial copy **before** a
  200-epoch run starts.

## Testing

`tests/train/test_stage.py`:

- tmp-dir fixture builds a fake dataset (tiny WAVs under `dry/`, `wet/` + a CSV with
  absolute paths + `manifest.csv`).
- `stage_zip` + `remap_manifests` → remapped manifests whose paths exist and load
  via `make_loader`.
- `stage_rclone` shells out, so test against a local `file:`-style source dir or
  monkeypatch the subprocess call and assert argv is `rclone copy ...` (never
  `mount`).

## Out of scope (YAGNI)

- No cross-runtime dataset caching, no partial / incremental copy.
- No parallelism tuning beyond rclone defaults.
- Notebooks stay two separate files (no merge).
