# Engram-Transform test data (Spec.md §7)

Inputs are pickle-wrapped raw bytes for the host's `pickle-bytes`
format; params are raw spec blobs (not pickled).

| file | dataset | programs |
| --- | --- | --- |
| synthetic_integer_records.pkl | synthetic integer records (2048 B) | count, filter, histogram, checksum |
| structured_logs.pkl | structured logs (2668 B) | checksum |
| chunked_binary.pkl | chunked binary records (4096 B) | checksum |
| count.params | count params (11 B) | count |
| filter.params | filter params (28 B) | filter |
| histogram.params | histogram params (35 B) | histogram |
| checksum.params | checksum params (2 B) | checksum |

Example:  executor count --input src/data/synthetic_integer_records.pkl --params src/data/count.params
