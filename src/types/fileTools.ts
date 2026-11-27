export interface FileToolsMount {
  readonly id: string
  readonly name: string
  readonly online: boolean
  readonly primary: boolean
}

export interface FileToolsMountListing {
  readonly mounts: readonly FileToolsMount[]
  readonly sharedUnavailable: boolean
}

export interface FileToolsScope {
  readonly mountId: string
  readonly path: string
}

export interface FileToolsEntryId {
  readonly scope: number
  readonly entry: number
}

export interface FileToolsEntry {
  readonly id: FileToolsEntryId
  readonly path: string
  readonly size: number
  readonly files: number
  readonly directory: boolean
  readonly canBrowse: boolean
  readonly complete: boolean
  readonly removable: boolean
}

export interface FileToolsCandidate {
  readonly size: number
  readonly estimatedBytes: number
  readonly failedEntries: number
  readonly entries: readonly FileToolsEntry[]
}

export interface FileToolsResults {
  readonly cachedFolders: number
  readonly mounts: readonly FileToolsMount[]
  readonly roots: readonly FileToolsEntry[]
  readonly emptyFolders: readonly FileToolsEntry[]
  readonly duplicates: readonly (readonly FileToolsEntry[])[]
  readonly candidates: readonly FileToolsCandidate[]
}

export interface FileToolsProgress {
  readonly folders: number
  readonly bytes: number
}
