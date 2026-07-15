class C:
    def method(self):
        return 42
    def other(self, x):
        return x * 2
m = C.method
print(m(C()))
print(C.other(C(), 5))
print(type(C.method).__name__)
