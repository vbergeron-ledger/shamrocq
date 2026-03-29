;; This extracted scheme code relies on some additional macros
;; available at http://www.pps.univ-paris-diderot.fr/~letouzey/scheme
(load "macros_extr.scm")


(define negb (lambda (b) (match b
                            ((True) `(False))
                            ((False) `(True)))))

(define length (lambda (l)
  (match l
     ((Nil) `(O))
     ((Cons _ l~) `(S ,(length l~))))))
  
(define leb (lambdas (n m)
  (match n
     ((O) `(True))
     ((S n~) (match m
                ((O) `(False))
                ((S m~) (@ leb n~ m~)))))))
  
(define map (lambdas (f l)
  (match l
     ((Nil) `(Nil))
     ((Cons a l0) `(Cons ,(f a) ,(@ map f l0))))))
  
(define fold_left (lambdas (f l a0)
  (match l
     ((Nil) a0)
     ((Cons b l0) (@ fold_left f l0 (@ f a0 b))))))
  
(define existsb (lambdas (f l)
  (match l
     ((Nil) `(False))
     ((Cons a l0) (match (f a)
                     ((True) `(True))
                     ((False) (@ existsb f l0)))))))
  
(define filter (lambdas (f l)
  (match l
     ((Nil) `(Nil))
     ((Cons x l0)
       (match (f x)
          ((True) `(Cons ,x ,(@ filter f l0)))
          ((False) (@ filter f l0)))))))
  
(define leb0 (lambdas (ord x y)
  (match (@ ord x y)
     ((Left) `(True))
     ((Right) `(False)))))

(define eqb (lambdas (ord x y)
  (match (@ leb0 ord x y)
     ((True) (@ leb0 ord y x))
     ((False) `(False)))))

(define ord_by (lambdas (h f x y) (@ h (f x) (f y))))

(define ord_eq_dec (lambdas (h x y)
  (let ((s (@ h x y)))
    (match s
       ((Left)
         (let ((s0 (@ h y x))) (match s0
                                  ((Left) `(Left))
                                  ((Right) `(Right)))))
       ((Right)
         (let ((s0 (@ h y x))) (match s0
                                  ((Left) `(Right))
                                  ((Right) `(Right)))))))))

(define merge_sorted (lambdas (h l1 l2)
  (letrec ((merge_right
          (lambda (l3)
          (match l1
             ((Nil) l3)
             ((Cons h1 t1)
               (match l3
                  ((Nil) l1)
                  ((Cons h2 t2)
                    (match (@ eqb h h1 h2)
                       ((True) `(Cons ,h1 ,(@ merge_sorted h t1 t2)))
                       ((False)
                         (match (@ leb0 h h1 h2)
                            ((True) `(Cons ,h1
                              ,(@ merge_sorted h t1 `(Cons ,h2 ,t2))))
                            ((False) `(Cons ,h2 ,(merge_right t2)))))))))))))
          (merge_right l2))))
  
(define dedup_sorted_aux (lambdas (h x l)
  (match l
     ((Nil) `(Cons ,x ,`(Nil)))
     ((Cons h0 t)
       (match (@ eqb h x h0)
          ((True) (@ dedup_sorted_aux h x t))
          ((False) `(Cons ,x ,(@ dedup_sorted_aux h h0 t))))))))
  
(define dedup_sorted (lambdas (h l)
  (match l
     ((Nil) `(Nil))
     ((Cons h0 t) (@ dedup_sorted_aux h h0 t)))))

(define merge_dedup_sorted (lambdas (h l1 l2)
  (@ dedup_sorted h (@ merge_sorted h l1 l2))))

(define ordRoot (lambda (h)
  (@ ord_by h (lambda (r) (match r
                             ((Build_root root_hash _) root_hash))))))

(define ordEdge (lambda (h)
  (@ ord_by h (lambda (e)
    (match e
       ((Build_edge _ edge_child_hash _) edge_child_hash))))))

(define hforest_init (lambdas (prev value prev_height) `(Build_hforest
  ,`(Cons ,`(Build_root ,prev ,prev_height) ,`(Nil)) ,`(Cons ,`(Build_edge
  ,prev ,value ,`(S ,prev_height)) ,`(Nil)))))

(define merge_roots (lambda (h) (merge_dedup_sorted (ordRoot h))))

(define merge_edges (lambda (h) (merge_dedup_sorted (ordEdge h))))

(define valid_root (lambdas (h children r)
  (negb
    (@ existsb (@ eqb h (match r
                           ((Build_root root_hash _) root_hash)))
      children))))

(define valid_roots (lambdas (h edges roots)
  (@ filter
    (@ valid_root h
      (@ map (lambda (e)
        (match e
           ((Build_edge _ edge_child_hash _) edge_child_hash)))
        edges))
    roots)))

(define hforest_merge (lambdas (h f1 f2)
  (let ((merged_edges
    (@ merge_edges h (match f1
                        ((Build_hforest _ edges) edges))
      (match f2
         ((Build_hforest _ edges) edges)))))
    (let ((merged_roots
      (@ merge_roots h (match f1
                          ((Build_hforest roots _) roots))
        (match f2
           ((Build_hforest roots _) roots)))))
      (let ((filtered_roots (@ valid_roots h merged_edges merged_roots)))
        `(Build_hforest ,filtered_roots ,merged_edges))))))

(define hforest_insert (lambdas (h prev value h0 f)
  (match (@ eqb h prev value)
     ((True) `(Pair ,f ,`(False)))
     ((False) `(Pair ,(@ hforest_merge h f (@ hforest_init prev value h0))
       ,`(True))))))

(define hforest_contains (lambdas (h x f)
  (match (@ existsb (@ eqb h x)
           (@ map (lambda (r) (match r
                                 ((Build_root root_hash _) root_hash)))
             (match f
                ((Build_hforest roots _) roots))))
     ((True) `(True))
     ((False)
       (@ existsb (@ eqb h x)
         (@ map (lambda (e)
           (match e
              ((Build_edge _ edge_child_hash _) edge_child_hash)))
           (match f
              ((Build_hforest _ edges) edges))))))))

(define is_tip (lambdas (h es e)
  (negb
    (@ existsb
      (@ eqb h (match e
                  ((Build_edge _ edge_child_hash _) edge_child_hash)))
      (@ map (lambda (e0)
        (match e0
           ((Build_edge edge_parent_hash _ _) edge_parent_hash)))
        es)))))

(define hforest_tips (lambdas (h f)
  (@ map (lambda (e) `(Pair
    ,(match e
        ((Build_edge _ edge_child_hash _) edge_child_hash))
    ,(match e
        ((Build_edge _ _ edge_child_height) edge_child_height))))
    (@ filter (@ is_tip h (match f
                             ((Build_hforest _ edges) edges)))
      (match f
         ((Build_hforest _ edges) edges))))))

(define prune_one_root (lambdas (h r f)
  (let ((rh (match r
               ((Build_root root_hash _) root_hash))))
    (let ((kept
      (@ filter (lambda (e)
        (negb
          (@ eqb h
            (match e
               ((Build_edge edge_parent_hash _ _) edge_parent_hash))
            rh)))
        (match f
           ((Build_hforest _ edges) edges)))))
      (let ((pruned
        (@ filter (lambda (e)
          (@ eqb h
            (match e
               ((Build_edge edge_parent_hash _ _) edge_parent_hash))
            rh))
          (match f
             ((Build_hforest _ edges) edges)))))
        (let ((new_roots
          (@ map (lambda (e) `(Build_root
            ,(match e
                ((Build_edge _ edge_child_hash _) edge_child_hash))
            ,(match e
                ((Build_edge _ _ edge_child_height) edge_child_height))))
            pruned)))
          (let ((old_roots
            (@ filter (lambda (r~)
              (negb
                (@ eqb h (match r~
                            ((Build_root root_hash _) root_hash)) rh)))
              (match f
                 ((Build_hforest roots _) roots)))))
            `(Build_hforest ,(@ merge_roots h old_roots new_roots) ,kept))))))))

(define prune_one (lambdas (h target f)
  (@ fold_left (lambdas (acc r) (@ prune_one_root h r acc))
    (@ filter (lambda (r)
      (@ leb (match r
                ((Build_root _ root_height) root_height)) target))
      (match f
         ((Build_hforest roots _) roots)))
    f)))

(define hforest_prune_aux (lambdas (h fuel target f)
  (match fuel
     ((O) f)
     ((S n)
       (match (@ existsb (lambda (r)
                (@ leb (match r
                          ((Build_root _ root_height) root_height))
                  target))
                (match f
                   ((Build_hforest roots _) roots)))
          ((True) (@ hforest_prune_aux h n target (@ prune_one h target f)))
          ((False) f))))))
  
(define hforest_prune (lambdas (h target f)
  (@ hforest_prune_aux h `(S
    ,(length (match f
                ((Build_hforest _ edges) edges))))
    target f)))
