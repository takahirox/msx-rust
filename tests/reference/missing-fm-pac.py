"""Original MIT Issue #75 regression sensitivity check.
Retain installation APIs but substitute the verbatim baseline Bus in a temporary
copy. This isolates missing external decoding without modifying production files.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
baseline = subprocess.check_output([
    'git', 'show', 'fb5e220db81a609fb8bbe0eab1cdd7696f589b9e:crates/msx-core/src/lib.rs',
], cwd=root, text=True)
start, end = 'impl Bus for Hardware {', 'pub struct Machine {'
old_bus = baseline[baseline.index(start):baseline.index(end)]
with tempfile.TemporaryDirectory(prefix='msx-missing-fm-pac-') as directory:
    directory = Path(directory)
    for name in ['crates', 'tests', 'firmware']:
        shutil.copytree(root / name, directory / name)
    for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']:
        shutil.copy(root / name, directory / name)
    path = directory / 'crates/msx-core/src/lib.rs'
    current = path.read_text()
    path.write_text(current[:current.index(start)] + old_bus + current[current.index(end):])
    env = os.environ.copy()
    sysroot = subprocess.check_output(['rustup', 'run', '1.99.0', 'rustc', '--print', 'sysroot'], text=True).strip()
    env['PATH'] = sysroot + '/bin:' + env['PATH']
    env['CARGO_TARGET_DIR'] = str(root / 'target/missing-fm-pac')
    for test in [
        'exact_four_banks_overlays_protection_unlocks_and_ignored_bits',
        'z80_discovery_banked_execution_sram_and_hidden_memory_programmed_sine_both_models',
    ]:
        result = subprocess.run(['cargo', 'test', '--locked', '-p', 'msx-core', '--test', 'fm_pac', test, '--', '--exact'],
                                cwd=directory, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode != 0 and 'panicked at' in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(test)
        print('\n'.join(line for line in result.stdout.splitlines()
                        if any(s in line for s in ['assertion', 'left:', 'right:', 'test result:'])))
print('Both assertions reject baseline external decoding. Production files unchanged.')
