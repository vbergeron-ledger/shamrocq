(define-foreign print-int)

(define fib (lambda (n)
  (if (< n 2)
    n
    (+ (fib (- n 1)) (fib (- n 2))))))

(define factorial (lambda (n)
  (if (< n 2)
    1
    (* n (factorial (- n 1))))))

(define range (lambdas (lo hi)
  (if (< lo hi)
    `(Cons ,lo ,(@ range (+ lo 1) hi))
    `(Nil))))

(define sum (lambda (l)
  (match l
    ((Nil) 0)
    ((Cons x xs) (+ x (sum xs))))))

(define map (lambdas (f l)
  (match l
    ((Nil) `(Nil))
    ((Cons x xs) `(Cons ,(f x) ,(@ map f xs))))))

(define length (lambda (l)
  (match l
    ((Nil) 0)
    ((Cons x xs) (+ 1 (length xs))))))
