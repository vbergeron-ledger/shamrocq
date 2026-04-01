Require Import Parser.Preamble.
Require Import Parser.Parser.

Extract Constant buffer    => "bytes".
Extract Constant buf_length => "(lambda (b) (bytes-len b))".
Extract Constant buf_get    => "(lambdas (b i) (bytes-get b i))".

Set Warnings "-extraction-default-directory".
Set Extraction Output Directory "../../../../examples/parser/scheme".

Extraction "parser.scm"
  parse_buffer
  count_tlvs
  read_u8
  read_u16_be
  read_bytes
  parse_tlv
  parse_all.
