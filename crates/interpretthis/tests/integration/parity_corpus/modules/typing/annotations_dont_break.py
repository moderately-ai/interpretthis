# Type annotations are stripped at runtime, so typing aliases don't
# need to do anything in expressions — they just need to exist as
# names. Pins that typing.List / typing.Dict / typing.Optional are
# resolvable as constants without erroring.
from typing import List, Dict, Optional

def add_items(xs, ys):
    return xs + ys

print(add_items([1, 2], [3, 4]))

# Annotated function signature — the annotations are parsed but not
# evaluated at runtime.
def labelled(d):
    total = 0
    for v in d.values():
        total += v
    return total

print(labelled({"a": 1, "b": 2, "c": 3}))
