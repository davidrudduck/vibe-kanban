/**
 * Organizations API - Organization management.
 */

import {
  ListOrganizationsResponse,
  OrganizationMemberWithProfile,
  ListMembersResponse,
  CreateOrganizationRequest,
  CreateOrganizationResponse,
  CreateInvitationRequest,
  CreateInvitationResponse,
  RevokeInvitationRequest,
  UpdateMemberRoleRequest,
  UpdateMemberRoleResponse,
  Invitation,
  RemoteProject,
  ListInvitationsResponse,
} from 'shared/types';

import { makeRequest, handleApiResponse } from './utils';

/**
 * Organizations API namespace - Organization management.
 */
export const organizationsApi = {
  getMembers: async (
    orgId: string
  ): Promise<OrganizationMemberWithProfile[]> => {
    const response = await makeRequest(`/api/organizations/${orgId}/members`);
    const result = await handleApiResponse<ListMembersResponse>(response);
    return result.members;
  },

  getUserOrganizations: async (): Promise<ListOrganizationsResponse> => {
    const response = await makeRequest('/api/organizations');
    return handleApiResponse<ListOrganizationsResponse>(response);
  },

  getProjects: async (orgId: string): Promise<RemoteProject[]> => {
    const response = await makeRequest(`/api/organizations/${orgId}/projects`);
    return handleApiResponse<RemoteProject[]>(response);
  },

  createOrganization: async (
    data: CreateOrganizationRequest
  ): Promise<CreateOrganizationResponse> => {
    const response = await makeRequest('/api/organizations', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    return handleApiResponse<CreateOrganizationResponse>(response);
  },

  createInvitation: async (
    orgId: string,
    data: CreateInvitationRequest
  ): Promise<CreateInvitationResponse> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<CreateInvitationResponse>(response);
  },

  removeMember: async (orgId: string, userId: string): Promise<void> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/members/${userId}`,
      {
        method: 'DELETE',
      }
    );
    return handleApiResponse<void>(response);
  },

  updateMemberRole: async (
    orgId: string,
    userId: string,
    data: UpdateMemberRoleRequest
  ): Promise<UpdateMemberRoleResponse> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/members/${userId}/role`,
      {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    );
    return handleApiResponse<UpdateMemberRoleResponse>(response);
  },

  listInvitations: async (orgId: string): Promise<Invitation[]> => {
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations`
    );
    const result = await handleApiResponse<ListInvitationsResponse>(response);
    return result.invitations;
  },

  revokeInvitation: async (
    orgId: string,
    invitationId: string
  ): Promise<void> => {
    const body: RevokeInvitationRequest = { invitation_id: invitationId };
    const response = await makeRequest(
      `/api/organizations/${orgId}/invitations/revoke`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }
    );
    return handleApiResponse<void>(response);
  },

  deleteOrganization: async (orgId: string): Promise<void> => {
    const response = await makeRequest(`/api/organizations/${orgId}`, {
      method: 'DELETE',
    });
    return handleApiResponse<void>(response);
  },
};
