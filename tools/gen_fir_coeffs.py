from scipy.signal import firwin

n = 63
beta = 8.6
cutoff = 0.5
h = firwin(n, cutoff, window=("kaiser", beta), pass_zero="lowpass")
h *= 2.0
print("const COEFFS: [f32; 63] = [")
for c in h:
    print(f"    {c:.10e},")
print("];")
