From Stdlib Require Import List Nat Bool.
Import ListNotations.

Fixpoint insert (n : nat) (l : list nat) : list nat :=
  match l with
  | [] => [n]
  | h :: t => if n <=? h then n :: l else h :: insert n t
  end.

Fixpoint insertion_sort (l : list nat) : list nat :=
  match l with
  | [] => []
  | h :: t => insert h (insertion_sort t)
  end.

Fixpoint merge (fuel : nat) (l1 l2 : list nat) : list nat :=
  match fuel with
  | 0 => l1 ++ l2
  | S fuel' =>
      match l1, l2 with
      | [], _ => l2
      | _, [] => l1
      | h1 :: t1, h2 :: t2 =>
          if h1 <=? h2
          then h1 :: merge fuel' t1 l2
          else h2 :: merge fuel' l1 t2
      end
  end.

Fixpoint split (l : list nat) : list nat * list nat :=
  match l with
  | [] => ([], [])
  | [x] => ([x], [])
  | x :: y :: rest =>
      let '(l1, l2) := split rest in
      (x :: l1, y :: l2)
  end.

Fixpoint merge_sort_aux (fuel : nat) (l : list nat) : list nat :=
  match fuel with
  | 0 => l
  | S fuel' =>
      match l with
      | [] => []
      | [x] => [x]
      | _ =>
          let '(l1, l2) := split l in
          merge fuel (merge_sort_aux fuel' l1) (merge_sort_aux fuel' l2)
      end
  end.

Definition merge_sort (l : list nat) : list nat :=
  let n := length l in
  merge_sort_aux n l.

Fixpoint seq (start len : nat) : list nat :=
  match len with
  | 0 => []
  | S n => start :: seq (S start) n
  end.

Fixpoint rev_range (n : nat) : list nat :=
  match n with
  | 0 => []
  | S m => n :: rev_range m
  end.

Definition sort_seq (n : nat) : list nat :=
  merge_sort (rev_range n).

Definition sort_insert_seq (n : nat) : list nat :=
  insertion_sort (rev_range n).
