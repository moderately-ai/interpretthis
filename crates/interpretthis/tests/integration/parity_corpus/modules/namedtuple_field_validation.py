# namedtuple validates field names: identifiers only, no keywords, no leading
# underscore, no duplicates, and the typename too. Regression: non-string fields
# were silently dropped and no validation ran.
from collections import namedtuple

P = namedtuple("P", ["x", "y"])
print(P(1, 2).x, P(1, 2).y, P._fields)
Q = namedtuple("Q", "a b c")          # space-separated
print(Q._fields)

for label, name, fields in [
    ("dup", "P", ["x", "x"]),
    ("keyword", "P", ["def", "y"]),
    ("nonident", "P", ["1x", "y"]),
    ("underscore", "P", ["_x", "y"]),
    ("bad_typename", "def", ["x"]),
    ("nonstr", "P", ["x", 1]),        # 1 -> "1" -> invalid identifier
]:
    try:
        namedtuple(name, fields)
    except ValueError:
        print(label, "ValueError")
