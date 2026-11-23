# Litematica: Java jar vs native rewrite

Perbandingan antara mod Java asli dan `ghost_litematic` (native `.hmod`) yang ada di project ini.

## Sumber yang dibandingkan

| | Java | Native rewrite |
|---|---|---|
| Package | `litematica-fabric-26.3-0.29.1.jar` + `malilib-fabric-26.3-0.30.2.jar` | `mods/ghost_litematic/` |
| Versi | Litematica 0.29.1, MaLiLib 0.30.2 (MC 26.3) | 0.9.1 |
| Ukuran | 504 class (Litematica) + 760 class (MaLiLib) | ~1 crate, ~1.000 baris Rust |
| Cara jalan | JVM + Fabric + Mixins ke kelas Minecraft | API mod Horizon (`mod_api`), wasm/native |
| Dependensi | `malilib >=0.30.2` | `horizon-mod-api`, `flate2` |

Dari jar: 96 class GUI, 41 class render/schematic, 12 class selection, 19 class placement, 7 class verifier, 18 class material list, dan Mixins di `block/`, `render/`, `easyplace/`, `network/`, `world/`, `entity/`, `model/`. MaLiLib menyediakan config system, hotkey, widget GUI, dan renderer helper.

## Status rewrite

Legenda:
- ✅ **Selesai** — ada di native mod dan sudah diuji.
- ◐ **Parsial** — ada versi sederhana, belum setara.
- ⛔ **Belum, bisa** — belum dibuat, tapi API saat ini memungkinkan.
- 🚧 **Belum, butuh API/engine baru** — tidak bisa hanya dengan API mod sekarang.
- ❌ **Tidak bisa client-only** — butuh mod server pendamping.
- ➖ **Tidak relevan** — konsep Fabric tidak dipakai Horizon.

### Schematic & data

| Fitur Java | Status | Keterangan |
|---|---|---|
| Baca `.litematic` (palette + `BlockStates`) | ✅ | Gzip NBT, named-root, bit-packed indices. |
| Baca `.schem` Sponge v1–3 | ✅ | Palette + LEB128 varint. |
| Pilih file lewat UI | ✅ | Config screen mendaftar file dari `data_files()`. |
| Simpan/export schematic | ⛔ Bisa | Perlu NBT writer; API sudah punya `Tag`. |
| Preview ImageData | ⛔ Bisa | Perlu dekode PNG + tampil di GUI. |
| DataFixer schema upgrade | 🚧 | Engine punya datafix, tapi tidak diekspos ke mod. |
| Block entity / entity di schematic | 🚧 | Tidak diekspos lewat `data_files()`; butuh parser + server untuk entity. |

### Rendering

| Fitur Java | Status | Keterangan |
|---|---|---|
| Ghost blok translucen | ◐ | 3 wajah kubus + shading; bukan model blok asli. |
| Model blok vanilla persis | 🚧 | `draw_model_instances` menolak material translucen; butuh API baru. |
| Wireframe region box | ✅ | `WorldDrawContext::lines`. |
| Layer/slice + follow player | ✅ | All/Single/Range/Follow. |
| Hide per region | ⛔ Bisa | Perlu state + UI tambahan. |
| Info HUD (blok, koordinat, material) | ◐ | HUD status sederhana saja. |
| Iris / Sodium / Jade compat | ➖ | Horizon punya pipeline sendiri. |

### Placement & editing

| Fitur Java | Status | Keterangan |
|---|---|---|
| Easy place (klik ghost) | ✅ | Pilot hotbar + arah + `key.use`. |
| Auto place | ✅ | 250 ms, bottom-up, berhenti saat habis. |
| Pick block dari ghost | ⛔ Bisa | `block_item` + hotbar scan sudah tersedia. |
| Placement restriction / warning | ⛔ Bisa | Perlu validasi item sebelum klik. |
| Paste schematic | ❌ | Client tidak punya `edit_blocks`; butuh mod server + payload. |
| Fill / replace / delete operation | ❌ | Sama: butuh server mod. |
| Command-based paste (setblock/fill) | ❌ | Client tidak bisa kirim perintah server secara bebas. |
| Servux / entity data sync | ❌ | Butuh server + protokol khusus. |

### Verifier

| Fitur Java | Status | Keterangan |
|---|---|---|
| Missing block | ✅ | Oranye. |
| Wrong block | ✅ | Merah. |
| Correct block | ✅ | Hijau. |
| Extra block (reverse diff) | ✅ | Magenta, dibatasi layer. |
| Wrong *state* vs wrong *block* | ⛔ Bisa | Bandingkan `block_state_string` lebih granular. |
| Block entity NBT check | 🚧 | NBT world tidak diekspos ke mod client. |

### GUI & kontrol

| Fitur Java (MaLiLib) | Status | Keterangan |
|---|---|---|
| Keybind bisa di-remap | ✅ | 21 `KeyRegistration` di Options > Controls. |
| Config screen | ◐ | Tombol + daftar file; belum tab/category/slider/color picker. |
| Multi-page mod menu | ⛔ Bisa | `Widgets` + `mod_screen_open` sudah ada. |
| Hotkey multi-key | ◐ | Satu mapping + modifier, belum chord. |
| Bahasa/terjemahan | ⛔ Bisa | Label sudah pakai translation key, belum ada file bahasa. |
| Color picker, slider, text field lanjutan | ⛔ Bisa | `TextField` ada, widget lain perlu dirakit manual. |

## Ringkasan

- **Sudah di-rewrite**: load schematic, ghost + wireframe, layer control, easy-place, auto-place, verifier 4 warna, config screen, keybind rebindable, anchor/pindah posisi.
- **Parsial**: model ghost (masih quads), GUI (masih satu layar), HUD (masih teks).
- **Bisa tapi belum dibuat**: export schematic, pick block, per-region hide, verifier wrong-state, GUI lengkap, i18n.
- **Butuh API/engine baru**: ghost model persis, block-entity NBT, datafixer.
- **Tidak mungkin client-only**: paste/fill/delete massal, entity spawn, Servux — semua butuh mod server pendamping dengan `edit_blocks` dan custom payload.

Secara jumlah fitur, rewrite ini baru mencakup sebagian kecil dari ~1.264 class Java, tetapi alur utama yang dipakai pemain (buka schematic → lihat ghost → atur layer → place/verifikasi) sudah berfungsi dan terverifikasi dengan e2e MCP.
