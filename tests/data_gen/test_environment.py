# tests/data_gen/test_environment.py
import re
import shutil
import subprocess


def test_ngspice_on_path():
    assert shutil.which("ngspice") is not None, "ngspice CLI must be installed"


def test_ngspice_version_at_least_42():
    out = subprocess.check_output(["ngspice", "--version"], text=True)
    match = re.search(r"ngspice-(\d+)", out)
    assert match is not None, f"could not locate ngspice version tag in:\n{out}"
    major = int(match.group(1))
    assert major >= 42, f"ngspice major version {major} < 42"
