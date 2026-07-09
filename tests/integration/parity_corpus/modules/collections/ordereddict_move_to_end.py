# OrderedDict.move_to_end — relocate a key to the back or front.
#
# Pins CPython semantics: last=True (default) sends the key to the
# end (most-recently-inserted slot); last=False sends to the front.
# Missing key raises KeyError.
from collections import OrderedDict

d = OrderedDict([("a", 1), ("b", 2), ("c", 3)])
print(list(d.keys()))

# Default: move to end.
d.move_to_end("a")
print(list(d.keys()))

# Positional False: move to front. (Method-call kwargs are not yet
# threaded through our dispatch; positional form behaves identically
# in CPython here so the test stays representative.)
d.move_to_end("c", False)
print(list(d.keys()))

# move_to_end is idempotent for the already-last key.
d2 = OrderedDict([("x", 1), ("y", 2)])
d2.move_to_end("y")
print(list(d2.keys()))

# Missing key raises KeyError.
try:
    d.move_to_end("missing")
except KeyError as e:
    print("KeyError")
