Require Import Eval.Preamble.
Require Import Eval.Eval.

Set Warnings "-extraction-default-directory".
Set Extraction Output Directory "../../../../examples/eval/scheme".

Extraction "eval.scm"
  whnf
  nf
  church
  church_add
  church_mul
  read_church
  test_add
  test_mul.
