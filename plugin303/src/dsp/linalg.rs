//! Fixed-size dense linear solve for the nodal Newton solvers (zero-alloc).
//! Const-generic LU with partial pivoting on stack arrays.

/// Solve `A·x = b` by Gaussian elimination with partial pivoting. Returns `None`
/// on a singular matrix. All storage is on the stack — no allocation.
pub fn lu_solve<const M: usize>(mut a: [[f64; M]; M], mut b: [f64; M]) -> Option<[f64; M]> {
    for col in 0..M {
        let mut piv = col;
        let mut max = a[col][col].abs();
        for r in (col + 1)..M {
            if a[r][col].abs() > max {
                max = a[r][col].abs();
                piv = r;
            }
        }
        if max < 1e-300 {
            return None;
        }
        if piv != col {
            a.swap(piv, col);
            b.swap(piv, col);
        }
        let d = a[col][col];
        for r in (col + 1)..M {
            let f = a[r][col] / d;
            if f != 0.0 {
                for c in col..M {
                    a[r][c] -= f * a[col][c];
                }
                b[r] -= f * b[col];
            }
        }
    }
    let mut x = [0.0; M];
    for i in (0..M).rev() {
        let mut s = b[i];
        for c in (i + 1)..M {
            s -= a[i][c] * x[c];
        }
        x[i] = s / a[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_2x2() {
        // [2 1; 1 3] x = [3; 5] -> x = [0.8; 1.4]
        let x = lu_solve::<2>([[2.0, 1.0], [1.0, 3.0]], [3.0, 5.0]).unwrap();
        assert!((x[0] - 0.8).abs() < 1e-12 && (x[1] - 1.4).abs() < 1e-12);
    }

    #[test]
    fn needs_pivot() {
        // Zero leading pivot forces a row swap.
        let x = lu_solve::<2>([[0.0, 1.0], [1.0, 0.0]], [2.0, 3.0]).unwrap();
        assert!((x[0] - 3.0).abs() < 1e-12 && (x[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn singular_returns_none() {
        assert!(lu_solve::<2>([[1.0, 2.0], [2.0, 4.0]], [1.0, 2.0]).is_none());
    }
}
