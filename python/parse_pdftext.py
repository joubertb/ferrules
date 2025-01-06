from pdftext.extraction import dictionary_output
from time import perf_counter

path = "/Users/amine/Downloads/RAG Corporate 2024 016.pdf"
s = perf_counter()
page_char_blocks = dictionary_output(
    path, keep_chars=False, workers=1, flatten_pdf=True, quote_loosebox=False
)
e = perf_counter()

print(f"Took : {1e3*(e - s):.2f}ms")
