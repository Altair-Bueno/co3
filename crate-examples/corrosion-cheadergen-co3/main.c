#include "corrosion_cheadergen_co3.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>

int main(void) {
  uint64_t sum = rust_super_safe_add(2, 40);
  printf("rust_super_safe_add(2, 40) = %" PRIu64 "\n", sum);
  return EXIT_SUCCESS;
}
