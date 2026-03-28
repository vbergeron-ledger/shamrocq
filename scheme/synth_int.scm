(define int_abs (lambda (n)
  (if (< n 0) (neg n) n)))

(define int_max (lambdas (a b)
  (if (< a b) b a)))

(define int_min (lambdas (a b)
  (if (< b a) b a)))

(define int_factorial (lambda (n)
  (if (< n 2) 1 (* n (int_factorial (- n 1))))))

(define int_sum_to (lambda (n)
  (if (= n 0) 0 (+ n (int_sum_to (- n 1))))))

(define int_pow (lambdas (base exp)
  (if (= exp 0) 1 (* base (@ int_pow base (- exp 1))))))

(define int_gcd (lambdas (a b)
  (if (= b 0) a (@ int_gcd b (- a (* (/ a b) b))))))

(define int_fib (lambda (n)
  (if (< n 2) n (+ (int_fib (- n 1)) (int_fib (- n 2))))))
