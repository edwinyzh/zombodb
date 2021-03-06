# Index Management

ZomboDB index management happens through standard Postgres DDL statements such as `CREATE INDEX`, `ALTER INDEX`, and `DROP INDEX`.  ZomboDB also exposes a number of index-level options that can be set to affect things like number of shards, replicas, etc.


## CREATE INDEX

The form for creating ZomboDB indices is:

```sql
CREATE INDEX index_name 
          ON table_name 
       USING zombodb ((table_name.*)) 
        WITH (...)
```
(where the options for `WITH` are detailed below)

ZomboDB generates a UUID to use as the backing Elasticsearch index name, but also assigns an alias in the form of `database_name.schema_name.table_name.index_name-index_oid`.  "index_oid" is the Postgres catalog id for the index from the "pg_class" system catalog table.

The alias is meant to be a human-readable name that you can use with external tools like Kibana or even curl.

## ALTER INDEX

The various Index Options supported by ZomboDB can be changed using Postgres `ALTER INDEX` statement.  They can be changed to new values or reset to their defaults.

For example:

```sql
ALTER INDEX index_name SET (replicas=2)
```

## DROP INDEX/TABLE/SCHEMA/DATABASE

When you drop a Postgres object that contains a ZomboDB index, the corresponding Elasticsearch is also deleted.

`DROP` statements are transaction safe and don't delete the backing Elasticsearch index until the controlling transaction commits.

Note that `DROP DATABASE` can't delete its corresponding Elasticsearch indices as there's no way for ZomboDB to receive a notification that a database is being dropped.

## WITH (...) Options

All of the below options can be set during `CREATE INDEX` and most of them can be changed with `ALTER INDEX`.  Those that cannot be altered are noted.

### Required Options

```
url

Type: string
Default: zdb.default_elasticsearch_url
```

The Elasticsearch Cluster URL for the index.  This option is required, but can be ommitted if the `postgresql.conf` setting `zdb.default_elasticsearch_url` is set.  This option can be changed with `ALTER INDEX`, but you must be a Postgres superuser to do so.

The value must end with a forward slash (`/`).


### Elasticsearch Options

```
shards

Type: integer
Default: 5
Range: [1, 32768]
```

The number of shards Elasticsearch should create for the index.  This option can be changed with `ALTER INDEX` but you must issue a `REINDEX INDEX` before the change will take effect.

```
replicas

Type: integer
Default: zdb.default_replicas
```

This controls the number of Elasticsearch index replicas.  The default is the value of the `zdb.default_replicas` GUC, which itself defaults to zero.  Changes to this value via `ALTER INDEX` take effect immediately.

```
alias

Type: string
Default: "database.schema.table.index-index_oid"
```

You can set an alias to use to identify an index from external tools.  This is for user convienece only.  Changes via `ALTER INDEX` take effect immediately.

Normal SELECT statements are executed in Elasticsearch directly against the named index.  Aggregate functions such as `zdb.count()` and `zdb.terms()` use the alias, however.  

In cases where you're using ZomboDB indices on inherited tables or on partition tables, it is suggested you assigned the **same** alias name to all tables in the hierarchy so that aggregate functions will run across all the tables involved.


```
refresh_interval

Type: string
Default: "-1"
```

This option specifies how frequently Elasticsearch should refresh the index to make changes visible to searches.  By default, this is set to `-1` because ZomboDB wants to control refreshes itself so that it can maintain proper MVCC visibility results.  It is not recommented that you change this setting unless you're okay with search results being inconsistent with what Postgres expects.  Changes via `ALTER INDEX` take effect immediately.

```
type_name

Type: string
Default: "doc"
```

This is the Elasticsearch index type into which documents are mapped.  The default, "doc" is compatible with Elasticsearch v5 and v6.  There should be no need to set.  Note that it can only be set during `CREATE INDEX`.


### Network Options

```
bulk_concurrency

Type: integer
Default: 12
Range: [1, 1024]
```

When synchronizing changes to Elasticsearch, ZomboDB does this by multiplexing HTTP(S) requests using libcurl.  This setting controls the number of concurrent requests.  ZomboDB also logs how many active concurrent requests it's managing during writes to Elasticsearch.  You can use that value to ensure you're not overloading your Elasticsearch cluster.  Changes via `ALTER INDEX` take effect immediately.

```
batch_size

Type: integer (in bytes)
Default: 8388608
Range: [1024, (INT_MAX/2)-1]
```

When synchronizing changes to Elasticsearch, ZomboDB does htis by batching them together into chunks of `batch_size`.  The default of 8mb is a sensible default, but can be changed in conjunction with `bulk_concurrency` to improve overall write performance.  Changes via `ALTER INDEX` take effect immediately.

```
compression_level

Type: integer
Default: 1
Range: [0, 9]
```

Sets the HTTP(s) transport (and request body) deflate compression level.  Over slow networks, it may make sense to set this to a higher value.  Setting to zero turns off all compression.  Changes via `ALTER INDEX` take effect immediately.


### Advanced Options

```
llapi

Type: boolean
Default: false
```

Indicates that this index will be used directly by ZomboDB's [low-level API](LLAPI.md).  Indices with this set to `true` will not have their corresponding Elasticsearch index deleted by `DROP INDEX/TABLE/SCHEMA`.