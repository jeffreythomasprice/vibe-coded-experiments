# Embedded PDF fonts

Compiled into the binary by `src/export/pdf.rs` (`include_bytes!`), so PDF export
works without system fonts.

- `NotoSans-Regular.ttf`: Noto Sans (latin-greek-cyrillic), from the Arch `noto-fonts` package.
- `NotoSansCJKjp-Regular.otf`: Noto Sans CJK JP, noto-cjk release `Sans2.004`
  (`Sans/OTF/Japanese/`). Covers kana, hangul and JP/SC/TC/KR ideographs.

Both are licensed under the SIL Open Font License 1.1 (`OFL.txt`).
