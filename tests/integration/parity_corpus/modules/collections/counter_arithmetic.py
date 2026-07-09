# Counter +/- multiset arithmetic: + adds counts, - subtracts.
# Both keep only strictly positive results. Pins counter_arith slot.
import collections
a = collections.Counter('aabbc')
b = collections.Counter('abbcd')
print(a + b)                    # {'a': 3, 'b': 4, 'c': 2, 'd': 1}
print(a - b)                    # {'a': 1, 'c': 0 -> dropped} -> {'a': 1}
print(b - a)                    # {'b': 0 -> dropped, 'c': 0 -> dropped, 'd': 1}
print(collections.Counter() + a)
print(a - a)                    # all zero -> empty Counter
