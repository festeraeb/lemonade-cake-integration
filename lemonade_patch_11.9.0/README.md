# Cake backend on Lemonade v11.9.0

`0001-cake-backend-on-v11.9.0.patch` applies to the upstream `v11.9.0` tree
(`git apply`, -p1). It adds the `cake|cake` entry to `LEMON_BACKENDS` and the
three backend files, identical to `lemonade_patch_applied_in_upstream_11.7.0`.
The adapter compiled against 11.9.0 without source changes
(`backend_descriptor.h` is unchanged between 11.7.0 and 11.9.0).

Build notes (CESARops c4): configure with `-DBUILD_WEB_APP=OFF` and
`PKG_CONFIG_PATH=/usr/lib/mbedtls3/pkgconfig`; the system mbedtls 4.x lacks the
headers cpp-httplib needs.

The descriptor declares no `support` rows, so the recipe is not listed in
`/api/v1/system-info`; it is accepted as a recipe by `/api/v1/pull`.
