# -*- mode: python ; coding: utf-8 -*-

a = Analysis(
  ['src/app.py'],
  pathex=[],
  binaries=[],
  datas=[('src/assets/setup_icon.png', '.'), ('src/assets/app_icon.png', '.'), ('src/assets/tray_icon.png', '.')]
  hiddenimports=[],
  hookspath=[],
  hooksconfig=[],
  runtime_hooks=[],
  excludes=[],
  noarchive=False,
)

pyz = PYZ(a.pure)

exe = EXE(
  pyz,
  a.scripts,
  [],
  exclude_binaries=True,
  name='Streaming Data Loader',
  debug=False,
  bootloader_ignore_signals=False,
  strip=False,
  upx=True,
  console=False,
  disable_windowed_traceback=False,
  argv_emulation=False,
  target_arch=False,
  codesign_identity=None,
  entitlements_file=None,
  icon=['src/assets/app_icon.png'],
)

coll = COLLECT(
  exe,
  a.binaries,
  a.datas,
  strip=False,
  upx=True,
  upx_exclude=[],
  name='Streaming Data Loader',
)
