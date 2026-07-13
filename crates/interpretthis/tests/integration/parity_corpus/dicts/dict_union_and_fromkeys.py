# Pins: PEP 584 dict union operator + dict.fromkeys constructor.
print(dict.fromkeys(['a', 'b', 'c'], 0))

a = {'x': 1}
b = {'y': 2}
print(a | b)
print({**a, **b})
