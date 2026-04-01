From Stdlib Require Extraction.

Extraction Language Scheme.

(* --- nat → native int --- *)
Extract Inductive nat => "int" [ "0" "(lambda (n) (+ n 1))" ]
  "(lambdas (fO fS n) (if (= n 0) (fO 0) (fS (- n 1))))".

Extract Constant Nat.add    => "(lambdas (n m) (+ n m))".
Extract Constant Nat.mul    => "(lambdas (n m) (* n m))".
Extract Constant Nat.sub    => "(lambdas (n m) (if (< n m) 0 (- n m)))".
Extract Constant Nat.div    => "(lambdas (n m) (if (= m 0) 0 (/ n m)))".
Extract Constant Nat.modulo => "(lambdas (n m) (if (= m 0) n (- n (* (/ n m) m))))".
Extract Constant Nat.eqb    => "(lambdas (n m) (if (= n m) `(True) `(False)))".
Extract Constant Nat.leb    => "(lambdas (n m) (if (< m n) `(False) `(True)))".
Extract Constant Nat.ltb    => "(lambdas (n m) (if (< n m) `(True) `(False)))".
