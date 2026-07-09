# Classic diamond: D inherits from B and C, both inherit from A. The C3
# MRO is [D, B, C, A] — depth-first left-to-right with each class
# appearing exactly once. Pins build_mro's merge step.
class A:
    def who(self):
        return "A"

class B(A):
    def who(self):
        return "B"

class C(A):
    def who(self):
        return "C"

class D(B, C):
    pass

d = D()
print(d.who())              # B (first in MRO after D)

# Cooperative chain: each level's method calls super().who(), so the
# whole MRO walks. D's super().who() resumes at B; B's super().who()
# resumes at C; C's super().who() resumes at A.
class B2(A):
    def who(self):
        return "B2->" + super().who()

class C2(A):
    def who(self):
        return "C2->" + super().who()

class D2(B2, C2):
    def who(self):
        return "D2->" + super().who()

print(D2().who())           # D2->B2->C2->A
