Require Import RBTree.Preamble.
Require Import RBTree.RBTree.

Set Warnings "-extraction-default-directory".
Set Extraction Output Directory "../../../../examples/rbtree/scheme".

Extraction "rbtree.scm"
  insert
  member
  insert_list
  all_member
  depth
  size
  build_tree
  build_and_check.
