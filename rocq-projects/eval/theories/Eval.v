From Stdlib Require Import Nat Bool List.
Import ListNotations.

Inductive term : Set :=
  | Var : nat -> term
  | Abs : term -> term
  | App : term -> term -> term.

Fixpoint shift (d c : nat) (t : term) : term :=
  match t with
  | Var n => if c <=? n then Var (n + d) else Var n
  | Abs body => Abs (shift d (S c) body)
  | App t1 t2 => App (shift d c t1) (shift d c t2)
  end.

Fixpoint subst (j : nat) (s t : term) : term :=
  match t with
  | Var n => if j =? n then s else Var n
  | Abs body => Abs (subst (S j) (shift 1 0 s) body)
  | App t1 t2 => App (subst j s t1) (subst j s t2)
  end.

Definition beta (body arg : term) : term :=
  shift 1 0 (subst 0 (shift 1 0 arg) body).

Fixpoint whnf (fuel : nat) (t : term) : option term :=
  match fuel with
  | 0 => None
  | S fuel' =>
      match t with
      | App t1 t2 =>
          match whnf fuel' t1 with
          | Some (Abs body) => whnf fuel' (beta body t2)
          | Some t1' => Some (App t1' t2)
          | None => None
          end
      | _ => Some t
      end
  end.

Fixpoint nf (fuel : nat) (t : term) : option term :=
  match fuel with
  | 0 => None
  | S fuel' =>
      match whnf fuel' t with
      | None => None
      | Some (Abs body) =>
          match nf fuel' body with
          | Some body' => Some (Abs body')
          | None => None
          end
      | Some (App t1 t2) =>
          match nf fuel' t1 with
          | Some t1' =>
              match nf fuel' t2 with
              | Some t2' => Some (App t1' t2')
              | None => None
              end
          | None => None
          end
      | Some t' => Some t'
      end
  end.

Definition church (n : nat) : term :=
  let fix go n :=
    match n with
    | 0 => Var 0
    | S m => App (Var 1) (go m)
    end
  in Abs (Abs (go n)).

Definition church_add : term :=
  Abs (Abs (Abs (Abs
    (App (App (Var 3) (Var 1))
         (App (App (Var 2) (Var 1)) (Var 0)))))).

Definition church_mul : term :=
  Abs (Abs (Abs
    (App (Var 2) (App (Var 1) (Var 0))))).

Fixpoint read_church (t : term) : option nat :=
  match t with
  | Abs (Abs body) =>
      let fix count t :=
        match t with
        | Var 0 => Some 0
        | App (Var 1) rest =>
            match count rest with
            | Some n => Some (S n)
            | None => None
            end
        | _ => None
        end
      in count body
  | _ => None
  end.

Definition test_add (a b fuel : nat) : option nat :=
  match nf fuel (App (App church_add (church a)) (church b)) with
  | Some t => read_church t
  | None => None
  end.

Definition test_mul (a b fuel : nat) : option nat :=
  match nf fuel (App (App church_mul (church a)) (church b)) with
  | Some t => read_church t
  | None => None
  end.
