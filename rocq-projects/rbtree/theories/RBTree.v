From Stdlib Require Import Nat Bool List.
Import ListNotations.

Inductive color : Set := Red | Black.

Inductive tree : Set :=
  | Leaf : tree
  | Node : color -> tree -> nat -> tree -> tree.

Fixpoint member (x : nat) (t : tree) : bool :=
  match t with
  | Leaf => false
  | Node _ l k r =>
      if x <? k then member x l
      else if k <? x then member x r
      else true
  end.

Definition balance (c : color) (l : tree) (k : nat) (r : tree) : tree :=
  match c, l, k, r with
  | Black, Node Red (Node Red a x b) y c_, z, d =>
      Node Red (Node Black a x b) y (Node Black c_ z d)
  | Black, Node Red a x (Node Red b y c_), z, d =>
      Node Red (Node Black a x b) y (Node Black c_ z d)
  | Black, a, x, Node Red (Node Red b y c_) z d =>
      Node Red (Node Black a x b) y (Node Black c_ z d)
  | Black, a, x, Node Red b y (Node Red c_ z d) =>
      Node Red (Node Black a x b) y (Node Black c_ z d)
  | c_, l_, k_, r_ => Node c_ l_ k_ r_
  end.

Fixpoint ins (x : nat) (t : tree) : tree :=
  match t with
  | Leaf => Node Red Leaf x Leaf
  | Node c l k r =>
      if x <? k then balance c (ins x l) k r
      else if k <? x then balance c l k (ins x r)
      else t
  end.

Definition make_black (t : tree) : tree :=
  match t with
  | Leaf => Leaf
  | Node _ l k r => Node Black l k r
  end.

Definition insert (x : nat) (t : tree) : tree :=
  make_black (ins x t).

Fixpoint insert_list (xs : list nat) (t : tree) : tree :=
  match xs with
  | [] => t
  | x :: rest => insert_list rest (insert x t)
  end.

Fixpoint all_member (xs : list nat) (t : tree) : bool :=
  match xs with
  | [] => true
  | x :: rest => andb (member x t) (all_member rest t)
  end.

Fixpoint depth (t : tree) : nat :=
  match t with
  | Leaf => 0
  | Node _ l _ r => 1 + max (depth l) (depth r)
  end.

Fixpoint size (t : tree) : nat :=
  match t with
  | Leaf => 0
  | Node _ l _ r => 1 + size l + size r
  end.

Fixpoint seq (start len : nat) : list nat :=
  match len with
  | 0 => []
  | S n => start :: seq (S start) n
  end.

Definition build_tree (n : nat) : tree :=
  insert_list (seq 0 n) Leaf.

Definition build_and_check (n : nat) : bool :=
  let t := build_tree n in
  all_member (seq 0 n) t.
