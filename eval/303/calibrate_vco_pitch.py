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
    out_file = root / "target" / "sweep_output.txt"
    with open(out_file, "w") as f:
        subprocess.run([str(bin_path)], stdout=f, check=True)
    
    with open(out_file, "r") as f:
        stdout_data = f.read()
    lines = stdout_data.strip().split("\n")
    data_by_topo = {"accurate": [], "fun": []}
    for line in lines:
        if not (line.startswith("accurate,") or line.startswith("fun,")):
            continue
        parts = line.split(",")
        if len(parts) == 4:
            topo = parts[0].strip()
            note = float(parts[1])
            nominal = float(parts[2])
            realized = float(parts[3])
            if topo in data_by_topo:
                data_by_topo[topo].append((note, nominal, realized))
                
    for topo, data in data_by_topo.items():
        print(f"\n--- Fitting {topo.upper()} ---")
        notes = np.array([d[0] for d in data])
        nominals = np.array([d[1] for d in data])
        realizeds = np.array([d[2] for d in data])
        
        dp = 12.0 * np.log2(realizeds / nominals)
        
        valid = (realizeds > 0) & (np.abs(dp) < 3.0)
        if topo == "accurate":
            valid &= (notes >= 24) & (notes <= 72)
        else:
            valid &= (notes >= 12) & (notes <= 108) # Note 0 can have numerical edge cases at low sample rates, let's fit 12-108
            
        if not np.any(valid):
            print(f"Warning: No valid pitch measurements found for {topo}!")
            continue
            
        notes_clean = notes[valid]
        dp_clean = dp[valid]
        print(f"Cleaned data points for {topo}:")
        for n, d in zip(notes_clean, dp_clean):
            if n in [24, 36, 48, 60, 72, 96, 108]:
                print(f"  Note {n}: dp={d:.4f} semitones")
        
        coeffs = np.polyfit(notes_clean, dp_clean, 3)
        
        print("Fitted 3rd-order polynomial coefficients [a3, a2, a1, a0]:")
        print(f"a3 = {coeffs[0]:.12e}")
        print(f"a2 = {coeffs[1]:.12e}")
        print(f"a1 = {coeffs[2]:.12e}")
        print(f"a0 = {coeffs[3]:.12e}")
        
        fit_vals = np.polyval(coeffs, notes_clean)
        residuals = dp_clean - fit_vals
        print(f"Max fit residual error: {np.max(np.abs(residuals)) * 100.0:.4f} cents")

if __name__ == "__main__":
    main()
