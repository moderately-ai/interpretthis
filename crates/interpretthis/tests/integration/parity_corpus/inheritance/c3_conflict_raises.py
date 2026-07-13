# C3 linearization cannot resolve an inconsistent inheritance graph.
# Here A and B disagree on the order of their bases (X then Y vs Y then
# X), so a class inheriting from both raises TypeError matching CPython
# exactly. Pins build_mro's "no good head" branch.
class X:
    pass

class Y:
    pass

class A(X, Y):
    pass

class B(Y, X):
    pass

try:
    class C(A, B):
        pass
    print("no error")
except TypeError as e:
    # CPython: "Cannot create a consistent method resolution order
    # (MRO) for bases ..." — we match the prefix exactly.
    print(str(e).startswith("Cannot create a consistent method resolution order"))
