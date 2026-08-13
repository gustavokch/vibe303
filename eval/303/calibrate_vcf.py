import subprocess
import sys
import numpy as np
from pathlib import Path
from scipy.optimize import minimize

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from parallel import run_parallel_with_fallback

HELPER = ROOT / "target" / "release" / "vcf_sweep_helper"

if not HELPER.exists():
    print(f"Error: helper binary not found at {HELPER}. Run 'cargo build --bin vcf_sweep_helper --release' first.")
    sys.exit(1)

def get_response(tier: str, fc: float, reso: float) -> np.ndarray:
    cmd = [
        str(HELPER),
        "--tier", tier,
        "--fc", f"{fc:.4f}",
        "--reso", f"{reso:.4f}",
        "--poles", "4"
    ]
    res = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return np.array([float(x) for x in res.stdout.strip().split()])

# Grid for calibration
FC_GRID = np.logspace(np.log10(200.0), np.log10(4000.0), 8)
RESO_GRID = np.array([0.0, 0.2, 0.4, 0.6, 0.8, 0.9])

def optimize_tier(tier: str):
    print(f"\n--- Optimizing {tier.upper()} Tier ---")
    
    tasks = []
    for fc_u in FC_GRID:
        for reso_u in RESO_GRID:
            tasks.append((fc_u, reso_u))
            
    def solve_point(fc_u: float, reso_u: float):
        target = get_response("ultra", fc_u, reso_u)
        
        def loss(params):
            fc_val, reso_val = params
            if fc_val < 20.0 or fc_val > 22000.0 or reso_val < 0.0 or reso_val > 1.2:
                return 1e9
            actual = get_response(tier, fc_val, reso_val)
            return np.mean((target - actual) ** 2)
            
        res = minimize(loss, [fc_u, reso_u], method='Nelder-Mead', options={'xatol': 1e-3, 'fatol': 1e-3})
        fc_opt, reso_opt = res.x
        return fc_u, reso_u, fc_opt, reso_opt, res.fun

    data_points = run_parallel_with_fallback(
        solve_point,
        tasks,
        desc=f"VCF {tier.upper()} optimization",
        max_workers=8
    )
    
    # Fit 2D Polynomials:
    # Inputs: X = log2(fc_ultra), Y = reso_ultra
    # Outputs: Z_fc = log2(fc_opt), Z_reso = reso_opt
    pts_clean = []
    for pt in data_points:
        fc_u, reso_u, fc_o, reso_o, l = pt
        if tier == "low" and (fc_o > 8000.0 or reso_o > 1.05):
            continue # Skip non-physical fits where TPT runs out of headroom
        pts_clean.append(pt)
        
    X = np.log2([pt[0] for pt in pts_clean])
    Y = np.array([pt[1] for pt in pts_clean])
    Z_fc = np.log2([pt[2] for pt in pts_clean])
    Z_reso = np.array([pt[3] for pt in pts_clean])
    
    # We fit a 2D polynomial of degree 3 (cubic, 10 terms):
    # f(x, y) = c0 + c1*x + c2*y + c3*x^2 + c4*x*y + c5*y^2 + c6*x^3 + c7*x^2*y + c8*x*y^2 + c9*y^3
    # terms: [1, x, y, x^2, x*y, y^2, x^3, x^2*y, x*y^2, y^3]
    A = np.column_stack([
        np.ones_like(X), 
        X, Y, 
        X**2, X*Y, Y**2,
        X**3, (X**2)*Y, X*(Y**2), Y**3
    ])
    
    # Solve via least-squares
    coeffs_fc, _, _, _ = np.linalg.lstsq(A, Z_fc, rcond=None)
    coeffs_reso, _, _, _ = np.linalg.lstsq(A, Z_reso, rcond=None)
    
    # Evaluate residuals
    fit_fc = A @ coeffs_fc
    fit_reso = A @ coeffs_reso
    
    err_fc_hz = np.abs(2.0**Z_fc - 2.0**fit_fc)
    err_reso = np.abs(Z_reso - fit_reso)
    
    print(f"\n--- Fit Results for {tier.upper()} Tier (Degree 3 Polynomial) ---")
    print("Coefficients for log2(fc_opt):")
    print(f"  c0 (const) = {coeffs_fc[0]:.12e}")
    print(f"  c1 (x)     = {coeffs_fc[1]:.12e}")
    print(f"  c2 (y)     = {coeffs_fc[2]:.12e}")
    print(f"  c3 (x^2)   = {coeffs_fc[3]:.12e}")
    print(f"  c4 (x*y)   = {coeffs_fc[4]:.12e}")
    print(f"  c5 (y^2)   = {coeffs_fc[5]:.12e}")
    print(f"  c6 (x^3)   = {coeffs_fc[6]:.12e}")
    print(f"  c7 (x^2*y) = {coeffs_fc[7]:.12e}")
    print(f"  c8 (x*y^2) = {coeffs_fc[8]:.12e}")
    print(f"  c9 (y^3)   = {coeffs_fc[9]:.12e}")
    print(f"Max fc fit error: {np.max(err_fc_hz):.2f} Hz, mean: {np.mean(err_fc_hz):.2f} Hz")
    
    print("\nCoefficients for reso_opt:")
    print(f"  d0 (const) = {coeffs_reso[0]:.12e}")
    print(f"  d1 (x)     = {coeffs_reso[1]:.12e}")
    print(f"  d2 (y)     = {coeffs_reso[2]:.12e}")
    print(f"  d3 (x^2)   = {coeffs_reso[3]:.12e}")
    print(f"  d4 (x*y)   = {coeffs_reso[4]:.12e}")
    print(f"  d5 (y^2)   = {coeffs_reso[5]:.12e}")
    print(f"  d6 (x^3)   = {coeffs_reso[6]:.12e}")
    print(f"  d7 (x^2*y) = {coeffs_reso[7]:.12e}")
    print(f"  d8 (x*y^2) = {coeffs_reso[8]:.12e}")
    print(f"  d9 (y^3)   = {coeffs_reso[9]:.12e}")
    print(f"Max reso fit error: {np.max(err_reso):.4f}, mean: {np.mean(err_reso):.4f}")

def main():
    optimize_tier("low")
    optimize_tier("high")

if __name__ == "__main__":
    main()
