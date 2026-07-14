# The `g`/`G` general float format: significant-digit precision, fixed vs
# scientific selection, and trailing-zero stripping.
for v in [3.0, 0.0001, 1000000.0, 0.00001, 123.456, 0.0, 1.5, 100.0, 1234567.0]:
    print(f"{v:g}")
print(f"{123.456:.2g}", f"{123.456:.4g}", f"{0.000123456:.3g}")
print(f"{1234.5:G}", f"{0.00001234:G}")
print(f"{1.0:#g}", f"{100:g}")
print("{:g}".format(2.5e-10), "{:.10g}".format(1.0 / 3.0))
