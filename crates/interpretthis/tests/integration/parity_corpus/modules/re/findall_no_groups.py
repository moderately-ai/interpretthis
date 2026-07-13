# Pins re.findall with 0 capture groups (whole-match list) against CPython.
import re
print(re.findall(r"\d+", "a1b22"))
print(re.findall(r"\w+", "hello world foo"))
print(re.findall(r"[a-z]+", "Abc Def Ghi"))
print(re.findall(r"\d+", "no digits here"))
print(re.findall(r"cat|dog", "a cat and a dog and a catfish"))
