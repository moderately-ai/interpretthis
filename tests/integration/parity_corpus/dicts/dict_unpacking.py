# Pins: dict literal with `**base` unpacking merges base then adds extras.
# In CPython 3.7+ insertion order is preserved, so the result is
# `{'a': 1, 'b': 2}`.
base = {'a': 1}
x = {**base, 'b': 2}
print(x)
