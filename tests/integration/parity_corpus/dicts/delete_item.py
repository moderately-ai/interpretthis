# Pins: `del d['a']` removes the key; the result dict still reprs deterministically.
d = {'a': 1, 'b': 2}
del d['a']
print(d)
