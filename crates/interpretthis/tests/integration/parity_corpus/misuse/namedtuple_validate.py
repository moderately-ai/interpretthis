from collections import namedtuple
try:
    P = namedtuple("P", ["x", "x"])
except ValueError as e:
    print("dup:", type(e).__name__)
try:
    Q = namedtuple("Q", ["1bad"])
except ValueError as e:
    print("ident:", type(e).__name__)
