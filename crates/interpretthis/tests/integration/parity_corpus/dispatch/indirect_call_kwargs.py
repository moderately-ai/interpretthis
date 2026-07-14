# A callable held in a variable keeps its keyword arguments. Regression:
# call_value_as_function took no kwargs, so every indirect call (a builtin, a
# bound method, a callable instance, a class classmethod) silently dropped them.
srt = sorted
print(srt([3, 1, 2], reverse=True))

upper_join = "-".join
print(upper_join(["a", "b", "c"]))

d = {"a": 1}
getter = d.get
print(getter("z", 99))


# A callable instance keeps its kwargs.
class Scale:
    def __call__(self, x, factor=1):
        return x * factor


s = Scale()
print(s(10, factor=3))


# A bound method with a keyword arg, held in a variable.
text = "hello"
padder = text.ljust
print(padder(8, "*"))
