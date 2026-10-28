The original project deliberately includes an unreachable module augmentation
that makes `Item.DEAD_MEMBER` required. The bundle drops that owner. The original
program is checked without the consumer; the consumer checks generated output
only, as in the former Rust regression. It verifies pruning, globals, companion
dependencies and the negative nested property assignment. Export names are still
compared between original and output programs.
