# Format width/alignment counts characters, not UTF-8 bytes. Regression: width
# used byte length, so a multi-byte subject was under-padded.
print(repr(f"{chr(233):>3}"))       # 'é' is 1 char -> two-space pad
print(repr(f"{chr(233) * 2:^6}"))   # centred within 6 chars
print(repr(f"{'café':8}"))          # 4 chars -> four trailing spaces
print(repr(f"{'café':>8}"))
print(repr(f"{'é':*^5}"))           # fill char, centred
print(repr(f"{'naïve':<7}!"))
print(repr(f"{'日本語':>5}"))        # CJK counts as 3 chars, pad 2
