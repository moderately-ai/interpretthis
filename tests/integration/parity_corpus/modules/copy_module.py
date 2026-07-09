# Pins: copy.copy and copy.deepcopy both return an independent clone.
# Mutations to the clone don't affect the original.
#
# Documented divergence: this interpreter's owned-value model makes
# copy.copy and copy.deepcopy semantically identical — there's no
# reference identity to share between shallow copies. CPython's
# `shallow[0].append(x)` mutates the original because shallow shares
# inner references; ours does not.
import copy

a = [1, 2, 3]
b = copy.copy(a)
b.append(4)
print(a)
print(b)

nested = {"k": [1, 2]}
deep = copy.deepcopy(nested)
deep["k"].append(99)
print(nested)
print(deep)
