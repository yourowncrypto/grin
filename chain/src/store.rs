// Copyright 2020 The Grin Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implements storage primitives required by the chain

use crate::core::consensus::HeaderInfo;
use crate::core::core::hash::{Hash, Hashed};
use crate::core::core::{Block, BlockHeader, BlockSums};
use crate::core::pow::Difficulty;
use crate::core::ser::ProtocolVersion;
use crate::types::{CommitPos, Tip};
use crate::util::secp::pedersen::Commitment;
use croaring::Bitmap;
use grin_store as store;
use grin_store::{option_to_not_found, Error, SerIterator};
use std::convert::TryInto;
use std::sync::Arc;

const STORE_SUBPATH: &str = "chain";

pub const SINGLE_KEY: &[u8] = b"K";

pub const BLOCK_HEADER: &str = "headers";
pub const BLOCK: &str = "blocks";
pub const HEAD: &str = "head";
pub const TAIL: &str = "tail";
pub const HEADER_HEAD: &str = "hhead";
pub const OUTPUT_POS: &str = "outpos";
pub const BLOCK_INPUT_BITMAP: &str = "bitmap";
pub const BLOCK_SUMS: &str = "sums";
pub const BLOCK_SPENT: &str = "spent";

/// All chain-related database operations
pub struct ChainStore {
	db: store::Store,
}

impl ChainStore {
	/// Create new chain store
	pub fn new(db_root: &str) -> Result<ChainStore, Error> {
		let db = store::Store::new(
			db_root,
			Some(STORE_SUBPATH),
			vec![
				BLOCK_HEADER,
				BLOCK,
				HEAD,
				TAIL,
				HEADER_HEAD,
				OUTPUT_POS,
				BLOCK_INPUT_BITMAP,
				BLOCK_SUMS,
				BLOCK_SPENT,
			],
			None,
		)?;
		Ok(ChainStore { db })
	}

	/// Create a new instance of the chain store based on this instance
	/// but with the provided protocol version. This is used when migrating
	/// data in the db to a different protocol version, reading using one version and
	/// writing back to the db with a different version.
	pub fn with_version(&self, version: ProtocolVersion) -> ChainStore {
		let db_with_version = self.db.with_version(version);
		ChainStore {
			db: db_with_version,
		}
	}
}

impl ChainStore {
	/// The current chain head.
	pub fn head(&self) -> Result<Tip, Error> {
		option_to_not_found(self.db.get_ser(HEAD, SINGLE_KEY), || "HEAD".to_owned())
	}

	/// The current header head (may differ from chain head).
	pub fn header_head(&self) -> Result<Tip, Error> {
		option_to_not_found(self.db.get_ser(HEADER_HEAD, SINGLE_KEY), || {
			"HEADER_HEAD".to_owned()
		})
	}

	/// The current chain "tail" (earliest block in the store).
	pub fn tail(&self) -> Result<Tip, Error> {
		option_to_not_found(self.db.get_ser(TAIL, SINGLE_KEY), || "TAIL".to_owned())
	}

	/// Header of the block at the head of the block chain (not the same thing as header_head).
	pub fn head_header(&self) -> Result<BlockHeader, Error> {
		self.get_block_header(&self.head()?.last_block_h)
	}

	/// Get full block.
	pub fn get_block(&self, h: &Hash) -> Result<Block, Error> {
		option_to_not_found(self.db.get_ser(BLOCK, h), || format!("BLOCK: {}", h))
	}

	/// Does this full block exist?
	pub fn block_exists(&self, h: &Hash) -> Result<bool, Error> {
		self.db.exists(BLOCK, h)
	}

	/// Get block_sums for the block hash.
	pub fn get_block_sums(&self, h: &Hash) -> Result<BlockSums, Error> {
		option_to_not_found(self.db.get_ser(BLOCK_SUMS, h), || {
			format!("Block sums for block: {}", h)
		})
	}

	/// Get previous header.
	pub fn get_previous_header(&self, header: &BlockHeader) -> Result<BlockHeader, Error> {
		self.get_block_header(&header.prev_hash)
	}

	/// Get block header.
	pub fn get_block_header(&self, h: &Hash) -> Result<BlockHeader, Error> {
		option_to_not_found(self.db.get_ser(BLOCK_HEADER, h), || {
			format!("BLOCK HEADER: {}", h)
		})
	}

	/// Get PMMR pos for the given output commitment.
	pub fn get_output_pos(&self, commit: &Commitment) -> Result<u64, Error> {
		match self.get_output_pos_height(commit)? {
			Some((pos, _)) => Ok(pos),
			None => Err(Error::NotFoundErr(format!(
				"Output position for: {:?}",
				commit
			))),
		}
	}

	/// Get PMMR pos and block height for the given output commitment.
	pub fn get_output_pos_height(&self, commit: &Commitment) -> Result<Option<(u64, u64)>, Error> {
		self.db.get_ser(OUTPUT_POS, commit)
	}

	/// Builds a new batch to be used with this store.
	pub fn batch(&self) -> Result<Batch<'_>, Error> {
		Ok(Batch {
			db: self.db.batch()?,
		})
	}
}

/// An atomic batch in which all changes can be committed all at once or
/// discarded on error.
pub struct Batch<'a> {
	db: store::Batch<'a>,
}

impl<'a> Batch<'a> {
	/// The head.
	pub fn head(&self) -> Result<Tip, Error> {
		option_to_not_found(self.db.get_ser(HEAD, SINGLE_KEY), || "HEAD".to_owned())
	}

	/// The tail.
	pub fn tail(&self) -> Result<Tip, Error> {
		option_to_not_found(self.db.get_ser(TAIL, SINGLE_KEY), || "TAIL".to_owned())
	}

	/// The current header head (may differ from chain head).
	pub fn header_head(&self) -> Result<Tip, Error> {
		option_to_not_found(self.db.get_ser(HEADER_HEAD, SINGLE_KEY), || {
			"HEADER_HEAD".to_owned()
		})
	}

	/// Header of the block at the head of the block chain (not the same thing as header_head).
	pub fn head_header(&self) -> Result<BlockHeader, Error> {
		self.get_block_header(&self.head()?.last_block_h)
	}

	/// Save body head to db.
	pub fn save_body_head(&self, t: &Tip) -> Result<(), Error> {
		self.db.put_ser(HEAD, SINGLE_KEY, t)
	}

	/// Save body "tail" to db.
	pub fn save_body_tail(&self, t: &Tip) -> Result<(), Error> {
		self.db.put_ser(TAIL, SINGLE_KEY, t)
	}

	/// Save header head to db.
	pub fn save_header_head(&self, t: &Tip) -> Result<(), Error> {
		self.db.put_ser(HEADER_HEAD, SINGLE_KEY, t)
	}

	/// get block
	pub fn get_block(&self, h: &Hash) -> Result<Block, Error> {
		option_to_not_found(self.db.get_ser(BLOCK, h), || {
			format!("Block with hash: {}", h)
		})
	}

	/// Does the block exist?
	pub fn block_exists(&self, h: &Hash) -> Result<bool, Error> {
		self.db.exists(BLOCK, h)
	}

	/// Save the block to the db.
	/// Note: the block header is not saved to the db here, assumes this has already been done.
	pub fn save_block(&self, b: &Block) -> Result<(), Error> {
		self.db.put_ser(BLOCK, b.hash(), b)?;
		Ok(())
	}

	/// We maintain a "spent" index for each full block to allow the output_pos
	/// to be easily reverted during rewind.
	pub fn save_spent_index(&self, h: &Hash, spent: &Vec<CommitPos>) -> Result<(), Error> {
		self.db.put_ser(BLOCK_SPENT, h, spent)?;
		Ok(())
	}

	/// Migrate a block stored in the db by serializing it using the provided protocol version.
	/// Block may have been read using a previous protocol version but we do not actually care.
	pub fn migrate_block(&self, b: &Block, version: ProtocolVersion) -> Result<(), Error> {
		self.db.put_ser_with_version(BLOCK, b.hash(), b, version)?;
		Ok(())
	}

	/// Low level function to delete directly by raw key.
	pub fn delete(&self, db: &str, key: &[u8]) -> Result<(), Error> {
		self.db.delete(db, key)
	}

	/// Delete a full block. Does not delete any record associated with a block
	/// header.
	pub fn delete_block(&self, bh: &Hash) -> Result<(), Error> {
		self.db.delete(BLOCK, bh)?;

		// Best effort at deleting associated data for this block.
		// Not an error if these fail.
		{
			let _ = self.delete_block_sums(bh);
			let _ = self.delete_spent_index(bh);
		}

		Ok(())
	}

	/// Save block header to db.
	pub fn save_block_header(&self, header: &BlockHeader) -> Result<(), Error> {
		let hash = header.hash();

		// Store the header itself indexed by hash.
		self.db.put_ser(BLOCK_HEADER, hash, header)?;
		Ok(())
	}

	/// Save output_pos and block height to index.
	pub fn save_output_pos_height(
		&self,
		commit: &Commitment,
		pos: u64,
		height: u64,
	) -> Result<(), Error> {
		self.db.put_ser(OUTPUT_POS, commit, &(pos, height))
	}

	/// Delete the output_pos index entry for a spent output.
	pub fn delete_output_pos_height(&self, commit: &Commitment) -> Result<(), Error> {
		self.db.delete(OUTPUT_POS, commit)
	}

	/// When using the output_pos iterator we have access to the index keys but not the
	/// original commitment that the key is constructed from. So we need a way of comparing
	/// a key with another commitment without reconstructing the commitment from the key bytes.
	pub fn is_match_output_pos_key(&self, key: &[u8], commit: &Commitment) -> bool {
		commit.as_ref() == key
	}

	/// Iterator over the output_pos index.
	pub fn output_pos_iter(&self) -> Result<SerIterator<(u64, u64)>, Error> {
		self.db.iter(OUTPUT_POS, SINGLE_KEY)
	}

	/// Get output_pos from index.
	pub fn get_output_pos(&self, commit: &Commitment) -> Result<u64, Error> {
		match self.get_output_pos_height(commit)? {
			Some((pos, _)) => Ok(pos),
			None => Err(Error::NotFoundErr(format!(
				"Output position for: {:?}",
				commit
			))),
		}
	}

	/// Get output_pos and block height from index.
	pub fn get_output_pos_height(&self, commit: &Commitment) -> Result<Option<(u64, u64)>, Error> {
		self.db.get_ser(OUTPUT_POS, commit)
	}

	/// Get the previous header.
	pub fn get_previous_header(&self, header: &BlockHeader) -> Result<BlockHeader, Error> {
		self.get_block_header(&header.prev_hash)
	}

	/// Get block header.
	pub fn get_block_header(&self, h: &Hash) -> Result<BlockHeader, Error> {
		option_to_not_found(self.db.get_ser(BLOCK_HEADER, h), || {
			format!("BLOCK HEADER: {}", h)
		})
	}

	/// Delete the block spent index.
	fn delete_spent_index(&self, bh: &Hash) -> Result<(), Error> {
		// Clean up the legacy input bitmap as well.
		let _ = self.db.delete(BLOCK_INPUT_BITMAP, bh);
		self.db.delete(BLOCK_SPENT, bh)
	}

	/// Save block_sums for the block.
	pub fn save_block_sums(&self, h: &Hash, sums: BlockSums) -> Result<(), Error> {
		self.db.put_ser(BLOCK_SUMS, h, &sums)
	}

	/// Get block_sums for the block.
	pub fn get_block_sums(&self, h: &Hash) -> Result<BlockSums, Error> {
		option_to_not_found(self.db.get_ser(BLOCK_SUMS, h), || {
			format!("Block sums for block: {}", h)
		})
	}

	/// Delete the block_sums for the block.
	fn delete_block_sums(&self, bh: &Hash) -> Result<(), Error> {
		self.db.delete(BLOCK_SUMS, bh)
	}

	/// Get the block input bitmap based on our spent index.
	/// Fallback to legacy block input bitmap from the db.
	pub fn get_block_input_bitmap(&self, bh: &Hash) -> Result<Bitmap, Error> {
		if let Ok(spent) = self.get_spent_index(bh) {
			let bitmap = spent
				.into_iter()
				.map(|x| x.pos.try_into().unwrap())
				.collect();
			Ok(bitmap)
		} else {
			self.get_legacy_input_bitmap(bh)
		}
	}

	fn get_legacy_input_bitmap(&self, bh: &Hash) -> Result<Bitmap, Error> {
		option_to_not_found(
			self.db
				.get_with(BLOCK_INPUT_BITMAP, bh, Bitmap::deserialize),
			|| "legacy block input bitmap".to_string(),
		)
	}

	/// Get the "spent index" from the db for the specified block.
	/// If we need to rewind a block then we use this to "unspend" the spent outputs.
	pub fn get_spent_index(&self, bh: &Hash) -> Result<Vec<CommitPos>, Error> {
		option_to_not_found(self.db.get_ser(BLOCK_SPENT, bh), || {
			format!("spent index: {}", bh)
		})
	}

	/// Commits this batch. If it's a child batch, it will be merged with the
	/// parent, otherwise the batch is written to db.
	pub fn commit(self) -> Result<(), Error> {
		self.db.commit()
	}

	/// Creates a child of this batch. It will be merged with its parent on
	/// commit, abandoned otherwise.
	pub fn child(&mut self) -> Result<Batch<'_>, Error> {
		Ok(Batch {
			db: self.db.child()?,
		})
	}

	/// An iterator to all block in db
	pub fn blocks_iter(&self) -> Result<SerIterator<Block>, Error> {
		self.db.iter(BLOCK, SINGLE_KEY)
	}
}

/// An iterator on blocks, from latest to earliest, specialized to return
/// information pertaining to block difficulty calculation (timestamp and
/// previous difficulties). Mostly used by the consensus next difficulty
/// calculation.
pub struct DifficultyIter<'a> {
	start: Hash,
	store: Option<Arc<ChainStore>>,
	batch: Option<Batch<'a>>,

	// maintain state for both the "next" header in this iteration
	// and its previous header in the chain ("next next" in the iteration)
	// so we effectively read-ahead as we iterate through the chain back
	// toward the genesis block (while maintaining current state)
	header: Option<BlockHeader>,
	prev_header: Option<BlockHeader>,
}

impl<'a> DifficultyIter<'a> {
	/// Build a new iterator using the provided chain store and starting from
	/// the provided block hash.
	pub fn from<'b>(start: Hash, store: Arc<ChainStore>) -> DifficultyIter<'b> {
		DifficultyIter {
			start,
			store: Some(store),
			batch: None,
			header: None,
			prev_header: None,
		}
	}

	/// Build a new iterator using the provided chain store batch and starting from
	/// the provided block hash.
	pub fn from_batch(start: Hash, batch: Batch<'_>) -> DifficultyIter<'_> {
		DifficultyIter {
			start,
			store: None,
			batch: Some(batch),
			header: None,
			prev_header: None,
		}
	}
}

impl<'a> Iterator for DifficultyIter<'a> {
	type Item = HeaderInfo;

	fn next(&mut self) -> Option<Self::Item> {
		// Get both header and previous_header if this is the initial iteration.
		// Otherwise move prev_header to header and get the next prev_header.
		self.header = if self.header.is_none() {
			if let Some(ref batch) = self.batch {
				batch.get_block_header(&self.start).ok()
			} else if let Some(ref store) = self.store {
				store.get_block_header(&self.start).ok()
			} else {
				None
			}
		} else {
			self.prev_header.clone()
		};

		// If we have a header we can do this iteration.
		// Otherwise we are done.
		if let Some(header) = self.header.clone() {
			if let Some(ref batch) = self.batch {
				self.prev_header = batch.get_previous_header(&header).ok();
			} else if let Some(ref store) = self.store {
				self.prev_header = store.get_previous_header(&header).ok();
			} else {
				self.prev_header = None;
			}

			let prev_difficulty = self
				.prev_header
				.clone()
				.map_or(Difficulty::zero(), |x| x.total_difficulty());
			let difficulty = header.total_difficulty() - prev_difficulty;
			let scaling = header.pow.secondary_scaling;

			Some(HeaderInfo::new(
				header.hash(),
				header.timestamp.timestamp() as u64,
				difficulty,
				scaling,
				header.pow.is_secondary(),
			))
		} else {
			None
		}
	}
}
