# Pins: dict.keys() / .values() / .items() iteration + len() +
# `in` membership. Heavy customer pattern in any dict-processing
# code.
d = {'a': 1, 'b': 2, 'c': 3}

for k in d:
    print(k)

print(list(d.keys()))
print(list(d.values()))
print(list(d.items()))

print(('a', 1) in d.items())
print(('a', 99) in d.items())

print(len(d.keys()))
print(len(d.values()))
print(len(d.items()))
