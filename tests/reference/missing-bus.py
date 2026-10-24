"""Original MIT regression sensitivity check for Issue #65.
Keep new installation/setup APIs, but replace Hardware's entire Bus implementation
with the verbatim missing-device baseline. No production files are modified.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
baseline = subprocess.check_output([
    'git', 'show', 'c54482e0153ad15db8e79888c81258515e085169:crates/msx-core/src/lib.rs',
], cwd=root, text=True)
start, end = 'impl Bus for Hardware {', 'pub struct Machine {'
old_bus = baseline[baseline.index(start):baseline.index(end)]
with tempfile.TemporaryDirectory(prefix='msx-missing-fm-bus-') as directory:
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
    env['CARGO_TARGET_DIR'] = str(root / 'target/missing-fm-bus')
    for test in [
        'original_z80_discovers_calls_fixture_and_programs_ports_while_rom_is_hidden',
        'reset_clears_chip_msx1_is_inactive_and_model_return_reactivates',
    ]:
        result = subprocess.run(['cargo', 'test', '-p', 'msx-core', '--test', 'msx_music', test, '--', '--exact'],
                                cwd=directory, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        assert result.returncode != 0 and 'panicked at' in result.stdout and 'test result: FAILED' in result.stdout, result.stdout
        print(test)
        print('\n'.join(line for line in result.stdout.splitlines()
                        if any(s in line for s in ['assertion', 'left:', 'right:', 'test result:'])))
print('Both assertions reject the verbatim baseline Bus. Production files unchanged.')
