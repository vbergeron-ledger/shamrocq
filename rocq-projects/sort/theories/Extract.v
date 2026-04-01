Require Import Sort.Preamble.
Require Import Sort.Sort.

Set Warnings "-extraction-default-directory".
Set Extraction Output Directory "../../../../examples/sort/scheme".

Extraction "sort.scm"
  merge_sort
  insertion_sort
  seq
  rev_range
  sort_seq
  sort_insert_seq.
