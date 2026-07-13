# Two-arg super(Cls, self) — explicit form. Pins the validation that
# arg2's class must have arg1 in its MRO.
class A:
    def label(self):
        return "A"

class B(A):
    def label(self):
        return "B+" + super(B, self).label()

class C(B):
    def label(self):
        return "C+" + super(C, self).label()

print(A().label())          # A
print(B().label())          # B+A
print(C().label())          # C+B+A
