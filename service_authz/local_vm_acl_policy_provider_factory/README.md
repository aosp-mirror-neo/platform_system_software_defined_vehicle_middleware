<!-- Copyright 2024 Google LLC -->

## Local VM ACL Policy Provider Factory

This directory has factory functions to create two different implementations of
SingleVmAclPolicyProvider for use by Service Discovery Agent on SDV or IVI.

### SDV ACL Policy Provider

The SDV provider reads [(package name, bundle name) -> ACL textproto path]
mappings from the Service Bundle Registry, which in turn reads them from APEXes.
This system is described more completely in go/sdv-authz-manifesto.

### IVI ACL Policy Provider

SBR does not run on IVI, so as an alternate solution the IVI provider scans a
set of predefined directories to find ACL policy textproto files. The scanned
directories are:

-   `/product/etc/sdv_authz_acls`
-   `/system/etc/sdv_authz_acls`
-   `/system_ext/etc/sdv_authz_acls`
-   `/vendor/etc/sdv_authz_acls`

The ACL textproto file for a given bundle must be placed in one of these
directories, with a relative path of the form
`<package_name>/<bundle_name>.textproto`.

So for example, the ACL for `my.package.MyBundle` could be placed at
`/product/etc/sdv_authz_acls/my.package/MyBundle.textproto`.

**Note:** In the event an ACL policy is found for the same bundle in more than
one of the root ACL directories, Service Discovery Agent will fail with an error
on startup.

### Build Configuration

A Soong variable `acl_provider_type` in the `sdv_authz` config namespace selects
which of these factory functions is compiled and used for a given product
target. One of these two lines should be added to the target's `BoardConfig.mk`
to select the correct provider:

```
$(call soong_config_set,sdv_authz,acl_provider_type,sdv_acl_provider)
```

```
$(call soong_config_set,sdv_authz,acl_provider_type,ivi_acl_provider)
```
