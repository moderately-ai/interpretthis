# pow(base, exp, mod) — integer modular exponentiation. Used in
# cryptographic code (RSA, Diffie-Hellman) and primality tests. Pins
# the square-and-multiply algorithm's correctness against CPython.
print(pow(2, 10))
print(pow(2, 10, 1000))
print(pow(2, 1000, 1000003))
print(pow(7, 100, 13))
print(pow(0, 5, 7))
print(pow(1, 100, 99))
print(pow(2, 3, -5))   # negative modulus: CPython matches modulus sign
