import subprocess
import numpy as np
import sys
from pathlib import Path

def main():
    root = Path(__file__).resolve().parent.parent.parent
    bin_path = root / "target" / "release" / "vco_pitch_sweep"
    if not bin_path.exists():
        bin_path = root / "target" / "debug" / "vco_pitch_sweep"
        
    print(f"Running VCO sweep from {bin_path}...")
    res = subprocess.run([str(bin_path)], capture_output=True, text=True, check=True)
    
    lines = res.stdout.strip().split("\n")
    data = []
    for line in lines[1:]:
        parts = line.split(",")
        if len(parts) == 3:
            note = float(parts[0])
            nominal = float(parts[1])
            realized = float(parts[2])
            data.append((note, nominal, realized))
            
    notes = np.array([d[0] for d in data])
    nominals = np.array([d[1] for d in data])
    realizeds = np.array([d[2] for d in data])
    
    dp = 12.0 * np.log2(realizeds / nominals)
    coeffs = np.polyfit(notes, dp, 3)
    
    print("\nFitted 3rd-order polynomial coefficients [a3, a2, a1, a0]:")
    print(f"a3 = {coeffs[0]:.12e}")
    print(f"a2 = {coeffs[1]:.12e}")
    print(f"a1 = {coeffs[2]:.12e}")
    print(f"a0 = {coeffs[3]:.12e}")
    
    fit_vals = np.polyval(coeffs, notes)
    residuals = dp - fit_vals
    print(f"\nMax fit residual error: {np.max(np.abs(residuals)) * 100.0:.4f} cents")

if __name__ == "__main__":
    main()
