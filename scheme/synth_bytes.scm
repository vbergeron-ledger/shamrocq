(define str_hello "hello")

(define str_empty "")

(define str_len (lambda (s) (bytes-len s)))

(define str_first (lambda (s) (bytes-get s 0)))

(define str_eq (lambdas (a b) (bytes-eq a b)))

(define str_cat (lambdas (a b) (bytes-cat a b)))

(define str_starts_with_h (lambda (s)
  (if (< (bytes-len s) 1)
    `(False)
    (if (= (bytes-get s 0) 104)
      `(True)
      `(False)))))
