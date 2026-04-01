From Stdlib Require Import Nat Bool List.
Import ListNotations.

Axiom buffer : Set.
Axiom buf_length : buffer -> nat.
Axiom buf_get : buffer -> nat -> nat.

Record pstate := mk_pstate { ps_buf : buffer ; ps_off : nat }.

Inductive parse_error : Set :=
  | UnexpectedEOF
  | BadTag (got : nat)
  | BadLength.

Definition parser (A : Type) : Type :=
  pstate -> (pstate * A) + parse_error.

Definition pure {A} (a : A) : parser A :=
  fun st => inl (st, a).

Definition fail {A} (e : parse_error) : parser A :=
  fun _ => inr e.

Definition bind {A B} (p : parser A) (f : A -> parser B) : parser B :=
  fun st =>
    match p st with
    | inl (st', a) => f a st'
    | inr e => inr e
    end.

Definition read_u8 : parser nat :=
  fun st =>
    if ps_off st <? buf_length (ps_buf st)
    then inl (mk_pstate (ps_buf st) (S (ps_off st)),
              buf_get (ps_buf st) (ps_off st))
    else inr UnexpectedEOF.

Definition read_u16_be : parser nat :=
  bind read_u8 (fun hi =>
  bind read_u8 (fun lo =>
  pure (hi * 256 + lo))).

Definition skip (n : nat) : parser unit :=
  fun st =>
    if ps_off st + n <=? buf_length (ps_buf st)
    then inl (mk_pstate (ps_buf st) (ps_off st + n), tt)
    else inr UnexpectedEOF.

Definition guard (b : bool) (e : parse_error) : parser unit :=
  if b then pure tt else fail e.

Definition at_end : parser bool :=
  fun st => inl (st, ps_off st =? buf_length (ps_buf st)).

Inductive tlv : Set :=
  | TlvU16   : nat -> tlv
  | TlvNested : list tlv -> tlv
  | TlvRaw   : list nat -> tlv.

Fixpoint read_bytes (n : nat) : parser (list nat) :=
  match n with
  | 0 => pure []
  | S m => bind read_u8 (fun b =>
           bind (read_bytes m) (fun rest =>
           pure (b :: rest)))
  end.

Fixpoint parse_tlv (fuel : nat) : parser tlv :=
  match fuel with
  | 0 => fail UnexpectedEOF
  | S fuel' =>
      bind read_u8 (fun tag =>
      bind read_u16_be (fun len =>
        if tag =? 1 then
          bind (guard (len =? 2) BadLength) (fun _ =>
          bind read_u16_be (fun v =>
          pure (TlvU16 v)))
        else if tag =? 2 then
          bind (parse_tlv_seq fuel' len) (fun items =>
          pure (TlvNested items))
        else if tag =? 3 then
          bind (read_bytes len) (fun bs =>
          pure (TlvRaw bs))
        else
          fail (BadTag tag)))
  end

with parse_tlv_seq (fuel : nat) (remaining : nat) : parser (list tlv) :=
  match fuel with
  | 0 => fail UnexpectedEOF
  | S fuel' =>
      if remaining =? 0 then pure []
      else
        fun st =>
          let start := ps_off st in
          match parse_tlv fuel' st with
          | inl (st', item) =>
              let consumed := ps_off st' - start in
              if remaining <? consumed then inr BadLength
              else
                match parse_tlv_seq fuel' (remaining - consumed) st' with
                | inl (st'', rest) => inl (st'', item :: rest)
                | inr e => inr e
                end
          | inr e => inr e
          end
  end.

Fixpoint parse_all (fuel : nat) : parser (list tlv) :=
  match fuel with
  | 0 => pure []
  | S fuel' =>
      bind at_end (fun done_ =>
        if done_ then pure []
        else bind (parse_tlv fuel') (fun item =>
             bind (parse_all fuel') (fun rest =>
             pure (item :: rest))))
  end.

Definition parse_buffer (buf : buffer) (fuel : nat) : (pstate * list tlv) + parse_error :=
  parse_all fuel (mk_pstate buf 0).

Fixpoint count_tlvs (items : list tlv) : nat :=
  match items with
  | [] => 0
  | _ :: rest => 1 + count_tlvs rest
  end.
