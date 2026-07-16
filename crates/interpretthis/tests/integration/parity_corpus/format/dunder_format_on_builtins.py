# Calling __format__ explicitly on a builtin value: empty spec is str(x), a
# non-empty spec runs the format-spec mini-language (same as format(x, spec)).
print((255).__format__("x"))
print((255).__format__(""))
print((42).__format__("05d"))
print((1234567).__format__(","))
print((3.14159).__format__(".2f"))
print((3.14159).__format__(""))
print("hi".__format__(">10"))
print("hi".__format__(""))
print((255).__format__("#x"))
print((5).__format__("08_b"))
print((True).__format__("d"))
print(format(255, "x") == (255).__format__("x"))
